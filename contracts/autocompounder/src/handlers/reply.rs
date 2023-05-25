use super::helpers::{
    convert_to_shares, cw20_total_supply, mint_vault_tokens, query_stake, stake_lp_tokens,
    swap_rewards_with_reply,
};
use crate::contract::{
    AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID,
    SWAPPED_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::response::MsgInstantiateContractResponse;
use crate::state::{
    Config,  CACHED_ASSETS, CACHED_FEE_AMOUNT, CACHED_USER_ADDR, CONFIG, FEE_CONFIG,
};
use abstract_cw_staking_api::{
    msg::{CwStakingQueryMsg, RewardTokensResponse},
    CW_STAKING,
};
use abstract_dex_api::api::DexInterface;
use abstract_dex_api::msg::OfferAsset;
use abstract_sdk::ApiInterface;
use abstract_sdk::{
    core::objects::{AnsAsset, AssetEntry, LpToken, PoolMetadata},
    features::AbstractResponse,
    features::{AbstractNameService, AccountIdentification},
    AbstractSdkResult, Resolve, TransferInterface,
};
use cosmwasm_std::{
    Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsg,
    Uint128,
};
use cw_asset::{Asset, AssetInfo};
use protobuf::Message;

/// Handle a reply for the [`INSTANTIATE_REPLY_ID`] reply.
pub fn instantiate_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    let vault_token_addr = res.get_contract_address();

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.vault_token = Addr::unchecked(vault_token_addr);
        Ok(config)
    })?;

    Ok(app.custom_tag_response(
        Response::new(),
        "instantiate",
        vec![("vault_token_addr", vault_token_addr)],
    ))
}

pub fn lp_provision_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let ans_host = app.ans_host(deps.as_ref())?;
    CACHED_USER_ADDR.remove(deps.storage);

    // get the total supply of Vault token
    let current_vault_supply = cw20_total_supply(deps.as_ref(), &config)?;

    // Retrieve the number of LP tokens minted/staked.
    let lp_token = LpToken::from(config.pool_data.clone());
    let received_lp = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy_address.to_string())?;

    // subtract the deposit fee from the received LP tokens
    let user_allocated_lp = received_lp.checked_sub(received_lp * fee_config.deposit)?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_token.clone().into(),
        config.unbonding_period,
    )?;

    // The increase in LP tokens held by the vault should be reflected by an equal increase (% wise) in vault tokens.
    // Calculate the number of vault tokens to mint
    let mint_amount = convert_to_shares(user_allocated_lp, staked_lp, current_vault_supply);
    if mint_amount.is_zero() {
        return Err(AutocompounderError::ZeroMintAmount {});
    }

    // Mint vault tokens to the user
    let mint_msg = mint_vault_tokens(&config, user_address, mint_amount)?;

    // Stake the LP tokens
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, received_lp), // stake the total amount of LP tokens received
        config.unbonding_period,
    )?;

    let res = Response::new().add_message(mint_msg).add_message(stake_msg);
    Ok(app.custom_tag_response(
        res,
        "lp_provision_reply",
        vec![("vault_token_minted", mint_amount)],
    ))
}

pub fn lp_withdrawal_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    let mut messages = vec![];
    let mut funds: Vec<AnsAsset> = vec![];

    for asset in config.pool_data.assets {
        let asset_info = asset.resolve(&deps.querier, &ans_host)?;
        let amount = asset_info.query_balance(&deps.querier, proxy_address.to_string())?;
        let prev_amount = CACHED_ASSETS.load(deps.storage, asset.to_string())?;
        let amount = amount.checked_sub(prev_amount)?;
        funds.push(AnsAsset::new(asset, amount));
    }
    CACHED_ASSETS.clear(deps.storage);

    let bank = app.bank(deps.as_ref());
    let transfer_msg = bank.transfer(funds, &user_address)?;
    messages.push(transfer_msg);

    let response = Response::new().add_messages(messages);
    Ok(app.tag_response(response, "lp_withdrawal_reply"))
}

pub fn lp_compound_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;

    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let mut messages = vec![];
    let mut submessages = vec![];
    // claim rewards (this happened in the execution before this reply)
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());

    // query the rewards and filters out zero rewards
    let mut rewards = get_staking_rewards(deps.as_ref(), &app, &config)?;

    if rewards.is_empty() {
        return Err(AutocompounderError::NoRewards {});
    }

    if !fee_config.performance.is_zero() {
        // deduct fee from rewards
        let fees = rewards
            .iter_mut()
            .map(|reward| -> AnsAsset {
                let fee = reward.amount * fee_config.performance;

                reward.amount -= fee;

                AnsAsset::new(reward.name.clone(), fee)
            })
            .filter(|fee| fee.amount > Uint128::zero())
            .collect::<Vec<AnsAsset>>();

        // Send fees to the fee collector
        if !fees.is_empty() {
            let transfer_msg = app
                .bank(deps.as_ref())
                .transfer(fees, &fee_config.fee_collector_addr)?;
            messages.push(transfer_msg);
        }
    }
    // Swap rewards to token in pool
    // check if asset is not in pool assets
    let pool_assets = config.pool_data.assets;
    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // if all assets are in the pool, we can just provide liquidity
        // The liquditiy assets are all the pool assets with the amount of the rewards
        let liquidity_assets = pool_assets
            .iter()
            .map(|pool_asset| -> AnsAsset {
                // Get the amount of the reward or return 0
                let amount = rewards
                    .iter()
                    .find(|reward| reward.name == *pool_asset)
                    .map(|reward| reward.amount)
                    .unwrap_or(Uint128::zero());
                OfferAsset::new(pool_asset.clone(), amount)
            })
            .collect::<Vec<OfferAsset>>();

        // provide liquidity
        let lp_msg: CosmosMsg =
            dex.provide_liquidity(liquidity_assets, Some(Decimal::percent(50)))?;

        submessages.push(SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID));

        let response = Response::new()
            .add_messages(messages)
            .add_submessages(submessages);

        Ok(app.tag_response(response, "provide_liquidity"))
    } else {
        let (swap_msgs, submsg) = swap_rewards_with_reply(
            rewards,
            pool_assets,
            &dex,
            SWAPPED_REPLY_ID,
            config.max_swap_spread,
        )?;
        messages.extend(swap_msgs);
        submessages.push(submsg);

        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        let response = Response::new()
            .add_messages(messages)
            .add_submessages(submessages);
        Ok(app.tag_response(response, "swap_rewards"))
    }
}

/// Queries the balances of pool assets and provides liquidity to the pool
///
/// This function is triggered after the last swap message of the lp_compound_reply
/// and assumes the contract has no other rewards than the ones in the pool assets
pub fn swapped_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let ans_host = app.ans_host(deps.as_ref())?;
    let config = CONFIG.load(deps.storage)?;
    let dex = app.dex(deps.as_ref(), config.pool_data.dex);

    // query balance of pool tokens
    let rewards = config
        .pool_data
        .assets
        .iter()
        .map(|entry| -> AbstractSdkResult<AnsAsset> {
            let tkn = entry.resolve(&deps.querier, &ans_host)?;
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;
            Ok(AnsAsset::new(entry.clone(), balance))
        })
        .collect::<AbstractSdkResult<Vec<AnsAsset>>>()?;

    // provide liquidity
    let lp_msg: CosmosMsg = dex.provide_liquidity(rewards, Some(Decimal::percent(10)))?;
    let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

    let response = Response::new().add_submessage(submsg);
    Ok(app.tag_response(response, "provide_liquidity"))
}

pub fn compound_lp_provision_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;
    let proxy = app.proxy_address(deps.as_ref())?;

    let lp_token = AssetEntry::from(LpToken::from(config.pool_data.clone()));

    // query balance of lp tokens
    let lp_balance = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy)?;

    // stake lp tokens
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        AnsAsset::new(lp_token, lp_balance),
        config.unbonding_period,
    )?;

    let response = Response::new().add_message(stake_msg);

    Ok(app.tag_response(response, "stake"))
}


fn query_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    pool_data: PoolMetadata,
) -> AbstractSdkResult<Vec<AssetInfo>> {
    // query staking module for which rewards are available
    let apis = app.apis(deps);
    let query = CwStakingQueryMsg::RewardTokens {
        provider: pool_data.dex.clone(),
        staking_token: LpToken::from(pool_data).into(),
    };
    let RewardTokensResponse { tokens } = apis.query(CW_STAKING, query)?;
    Ok(tokens)
}

/// queries available staking rewards assets and the corresponding balances
fn get_staking_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    config: &Config,
) -> AbstractSdkResult<Vec<AnsAsset>> {
    let ans_host = app.ans_host(deps)?;
    let rewards = query_rewards(deps, app, config.pool_data.clone())?;
    // query balance of rewards
    let rewards = rewards
        .into_iter()
        .map(|tkn| -> AbstractSdkResult<Asset> {
            //  get the number of LP tokens minted in this transaction
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps)?)?;
            Ok(Asset::new(tkn, balance))
        })
        .collect::<AbstractSdkResult<Vec<Asset>>>()?;
    // resolve rewards to AnsAssets for dynamic processing (swaps)
    let rewards = rewards
        .into_iter()
        .filter(|reward| reward.amount != Uint128::zero())
        .map(|asset| asset.resolve(&deps.querier, &ans_host))
        .collect::<Result<Vec<AnsAsset>, _>>()?;
    Ok(rewards)
}

#[cfg(test)]
mod test {}
