use super::helpers::{cw20_total_supply, query_stake};
use crate::contract::{
    AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID, FEE_SWAPPED_REPLY,
    SWAPPED_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::response::MsgInstantiateContractResponse;
use crate::state::{Config, CACHED_USER_ADDR, CONFIG};
use abstract_sdk::base::features::AbstractResponse;
use abstract_sdk::{
    apis::dex::{Dex, DexInterface},
    base::features::{AbstractNameService, Identification},
    os::objects::{AnsAsset, AssetEntry, LpToken, PoolMetadata},
    ModuleInterface, Resolve, TransferInterface,
};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, Reply, Response, StdError, StdResult,
    SubMsg, Uint128, WasmMsg,
};
use cw20_base::msg::ExecuteMsg::Mint;
use cw_asset::{Asset, AssetInfo};
use cw_utils::Duration;
use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, RewardTokensResponse, CW_STAKING,
};
use protobuf::Message;

/// Handle a relpy for the [`INSTANTIATE_REPLY_ID`] reply.
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
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let _ans_host = app.ans_host(deps.as_ref())?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let current_vault_supply = cw20_total_supply(deps.as_ref(), &config)?;

    // 2) Retrieve the number of LP tokens minted/staked.
    let lp_token = LpToken::from(config.pool_data.clone());
    let received_lp = lp_token
        .resolve(&deps.querier, &_ans_host)?
        .query_balance(&deps.querier, proxy_address.to_string())?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_token.clone().into(),
        config.unbonding_period,
    )?;

    // The increase in LP tokens held by the vault should be reflected by an equal increase (% wise) in vault tokens.
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = compute_mint_amount(staked_lp, current_vault_supply, received_lp);

    // 4) Mint vault tokens to the user
    let mint_msg = mint_vault_tokens(&config, user_address, mint_amount)?;

    // 5) Stake the LP tokens
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, received_lp),
        config.unbonding_period,
    )?;

    let res = Response::new().add_message(mint_msg).add_message(stake_msg);
    Ok(app.custom_tag_response(
        res,
        "lp_provision_reply",
        vec![("vault_token_minted", mint_amount)],
    ))
}

fn mint_vault_tokens(
    config: &Config,
    user_address: Addr,
    mint_amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    let mint_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.vault_token.to_string(),
        msg: to_binary(&Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        })?,
        funds: vec![],
    }
    .into();
    Ok(mint_msg)
}

fn compute_mint_amount(
    staked_lp: Uint128,
    current_vault_supply: Uint128,
    received_lp: Uint128,
) -> Uint128 {
    if !staked_lp.is_zero() {
        // will zero if first deposit
        current_vault_supply
            .checked_multiply_ratio(received_lp, staked_lp)
            .unwrap()
    } else {
        // if first deposit, mint the same amount of tokens as the LP tokens received
        received_lp
    }
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
        funds.push(AnsAsset::new(asset, amount));
    }

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
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());

    let base_state = app.load_state(deps.storage)?;
    let _proxy = base_state.proxy_address;
    // 1) claim rewards (this happened in the execution before this reply)

    // 2.1) query the rewards
    let mut rewards = get_staking_rewards(deps.as_ref(), &app, &config)?;

    // 2) deduct fee from rewards
    let fees = rewards
        .iter_mut()
        .map(|reward| -> StdResult<AnsAsset> {
            let fee = reward.amount * config.fees.performance;

            reward.amount -= fee;

            Ok(AnsAsset::new(reward.name.clone(), fee))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;

    // 3) (swap and) Send fees to treasury
    let (fee_swap_msgs, fee_swap_submsg) =
        swap_rewards_with_reply(fees, vec![config.fees.fee_asset], &dex, FEE_SWAPPED_REPLY)?;

    // 3) Swap rewards to token in pool
    // 3.1) check if asset is not in pool assets
    let pool_assets = config.pool_data.assets;
    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // 3.1.1) if all assets are in the pool, we can just provide liquidity
        //  TODO: but we might need to check the length of the rewards.

        // 3.1.2) provide liquidity
        let lp_msg: CosmosMsg = dex.provide_liquidity(rewards, Some(Decimal::percent(50)))?;

        let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

        Ok(Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_submessage(submsg)
            .add_attribute("action", "provide_liquidity"))
    } else {
        let (swap_msgs, submsg) =
            swap_rewards_with_reply(rewards, pool_assets, &dex, SWAPPED_REPLY_ID)?;

        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        let response = Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_messages(swap_msgs)
            .add_submessage(submsg);
        Ok(app.tag_response(response, "swap_rewards"))
    }
    // TODO: stake lp tokens
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

    // 1) query balance of pool tokens
    let rewards = config
        .pool_data
        .assets
        .iter()
        .map(|entry| -> StdResult<AnsAsset> {
            let tkn = entry.resolve(&deps.querier, &ans_host)?;
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;
            Ok(AnsAsset::new(entry.clone(), balance))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;

    // 2) provide liquidity
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

    // 1) query balance of lp tokens
    let lp_balance = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy)?;

    // 2) stake lp tokens
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

pub fn fee_swapped_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_asset = config.fees.fee_asset;

    let fee_balance = fee_asset
        .resolve(&deps.querier, &app.ans_host(deps.as_ref())?)?
        .query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;

    let transfer_msg = app.bank(deps.as_ref()).transfer(
        vec![AnsAsset::new(fee_asset, fee_balance)],
        &config.commission_addr,
    )?;

    let response = Response::new().add_message(transfer_msg);
    Ok(app.tag_response(response, "transfer_platform_fees"))
}

fn query_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    pool_data: PoolMetadata,
) -> StdResult<Vec<AssetInfo>> {
    // query staking module for which rewards are available
    let modules = app.modules(deps);
    let query = CwStakingQueryMsg::RewardTokens {
        provider: pool_data.dex.clone(),
        staking_token: LpToken::from(pool_data).into(),
    };
    let RewardTokensResponse { tokens } = modules.query_api(CW_STAKING, query)?;

    Ok(tokens)
}

// TODO: move to cw_staking SDK
fn stake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    asset: AnsAsset,
    unbonding_period: Option<Duration>,
) -> StdResult<CosmosMsg> {
    let modules = app.modules(deps);
    modules.api_request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Stake {
                staking_token: asset,
                unbonding_period,
            },
        },
    )
}

/// swaps all rewards that are not in the target assets and add a reply id to the latest swapmsg
fn swap_rewards_with_reply(
    rewards: Vec<AnsAsset>,
    target_assets: Vec<AssetEntry>,
    dex: &Dex<AutocompounderApp>,
    reply_id: u64,
) -> Result<(Vec<CosmosMsg>, SubMsg), AutocompounderError> {
    let mut swap_msgs: Vec<CosmosMsg> = vec![];
    rewards
        .iter()
        .try_for_each(|reward: &AnsAsset| -> StdResult<_> {
            if !target_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = dex.swap(
                    reward.clone(),
                    target_assets.get(0).unwrap().clone(),
                    Some(Decimal::percent(50)),
                    None,
                )?;
                swap_msgs.push(swap_msg);
            }
            Ok(())
        })?;
    let swap_msg = swap_msgs.pop().unwrap();
    let submsg = SubMsg::reply_on_success(swap_msg, reply_id);
    Ok((swap_msgs, submsg))
}

/// queries available staking rewards assets and the corresponding balances
fn get_staking_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    config: &Config,
) -> StdResult<Vec<AnsAsset>> {
    let ans_host = app.ans_host(deps)?;
    let rewards = query_rewards(deps, app, config.pool_data.clone())?;
    // query balance of rewards
    let rewards = rewards
        .into_iter()
        .map(|tkn| -> StdResult<Asset> {
            // 2) get the number of LP tokens minted in this transaction
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps)?)?;
            Ok(Asset::new(tkn, balance))
        })
        .collect::<StdResult<Vec<Asset>>>()?;
    // resolve rewards to AnsAssets for dynamic processing (swaps)
    let rewards = rewards
        .into_iter()
        .filter(|reward| reward.amount != Uint128::zero())
        .map(|asset| asset.resolve(&deps.querier, &ans_host))
        .collect::<Result<Vec<AnsAsset>, _>>()?;
    Ok(rewards)
}
