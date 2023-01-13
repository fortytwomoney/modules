use abstract_sdk::apis::modules::Modules;
use abstract_sdk::base::features::{AbstractNameService, Identification};

use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};
use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken, PoolMetadata};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsg,
    Uint128, WasmMsg,
};
use cw20_base::msg::ExecuteMsg::Mint;

use forty_two::cw_staking::{CwStakingAction, CwStakingExecuteMsg, CW_STAKING};

use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID, FEE_SWAPPED_REPLY,
    SWAPPED_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::state::{Config, CACHED_USER_ADDR, CONFIG};

use crate::response::MsgInstantiateContractResponse;

use super::helpers::{cw20_total_supply, query_stake};

/// Handle a relpy for the [`INSTANTIATE_REPLY_ID`] reply.
pub fn instantiate_reply(
    deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
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

    Ok(Response::new().add_attribute("vault_token_addr", vault_token_addr))
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
    )?;

    // The increase in LP tokens held by the vault should be reflected by an equal increase (% wise) in vault tokens.
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = if !staked_lp.is_zero() {
        // will zero if first deposit
        current_vault_supply
            .checked_multiply_ratio(received_lp, staked_lp)
            .unwrap()
    } else {
        // if first deposit, mint the same amount of tokens as the LP tokens received
        received_lp
    };

    // 4) Mint vault tokens to the user
    let mint_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.vault_token.to_string(),
        msg: to_binary(&Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        })?,
        funds: vec![],
    }
    .into();

    // 5) Stake the LP tokens
    let stake_msg = stake_lp_tokens(
        deps,
        app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, received_lp),
    )?;

    Ok(Response::new()
        .add_message(mint_msg)
        .add_message(stake_msg)
        .add_attribute("vault_token_minted", mint_amount))
}

pub fn lp_withdrawal_reply(
    deps: DepsMut,
    _env: Env,
    dapp: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = dapp.ans_host(deps.as_ref())?;
    let proxy_address = dapp.proxy_address(deps.as_ref())?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    let mut messages = vec![];
    let mut funds: Vec<AnsAsset> = vec![];
    for asset in config.pool_data.assets {
        let asset_info = asset.resolve(&deps.querier, &ans_host)?;
        let amount = asset_info.query_balance(&deps.querier, proxy_address.to_string())?;
        funds.push(AnsAsset::new(asset, amount));
    }

    let bank = dapp.bank(deps.as_ref());
    let transfer_msg = bank.transfer(funds, &user_address)?;
    messages.push(transfer_msg);

    Ok(Response::new().add_messages(messages))
}

pub fn lp_compound_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let modules = app.modules(deps.as_ref());

    let config = CONFIG.load(deps.storage)?;
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
    let (fee_swap_msgs, fee_swap_submsg) = swap_rewards_with_reply(
        fees,
        vec![config.fees.fee_asset],
        &modules,
        &config.pool_data.dex,
        FEE_SWAPPED_REPLY,
    )?;
    // - if we want to swap, we should just create swap msgs with the last one containing a reply id
    //   and then send the fees to the treasury in the reply
    // let fee_transfer_msg = bank.transfer(fees, &config.commission_addr)?;

    // 3) Swap rewards to token in pool
    // 3.1) check if asset is not in pool assets
    let pool_assets = config.pool_data.assets;
    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // 3.1.1) if all assets are in the pool, we can just provide liquidity
        //  TODO: but we might need to check the length of the rewards.

        // 3.1.2) provide liquidity
        let lp_msg: CosmosMsg = modules.api_request(
            EXCHANGE,
            DexExecuteMsg {
                dex: config.pool_data.dex,
                action: DexAction::ProvideLiquidity {
                    assets: rewards,
                    max_spread: None,
                },
            },
        )?;

        let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

        Ok(Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_submessage(submsg)
            .add_attribute("action", "provide_liquidity"))
    } else {
        let (swap_msgs, submsg) = swap_rewards_with_reply(
            rewards,
            pool_assets,
            &modules,
            &config.pool_data.dex,
            SWAPPED_REPLY_ID,
        )?;

        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        Ok(Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_messages(swap_msgs)
            .add_submessage(submsg)
            .add_attribute("action", "swap_rewards"))
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
    let modules = app.modules(deps.as_ref());
    let config = CONFIG.load(deps.storage)?;

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
    let lp_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.pool_data.dex,
            action: DexAction::ProvideLiquidity {
                assets: rewards,
                max_spread: None,
            },
        },
    )?;
    let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

    Ok(Response::new()
        .add_submessage(submsg)
        .add_attribute("action", "provide_liquidity"))
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
        deps,
        app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, lp_balance),
    )?;

    Ok(Response::new()
        .add_message(stake_msg)
        .add_attribute("action", "stake"))
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

    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attribute("action", "transfer_platfrom_fees"))
}

fn query_rewards(deps: Deps, app: &AutocompounderApp, _pool_data: PoolMetadata) -> Vec<AssetEntry> {
    // query staking module for which rewards are available
    let _modules = app.modules(deps);

    // TODO: Reward query has yet to be implemented
    // let query = CwStakingQueryMsg::Rewards {
    //     address: app.proxy_address(deps).unwrap().to_string(),
    //     pool_data,
    // };
    // let res: Vec<AssetEntry> = modules.query_api(CW_STAKING, query).unwrap();
    let res: Vec<AssetEntry> = vec![];

    res
}

// TODO: move to cw_staking SDK
fn stake_lp_tokens(
    deps: DepsMut,
    app: AutocompounderApp,
    provider: String,
    asset: AnsAsset,
) -> StdResult<CosmosMsg> {
    let modules = app.modules(deps.as_ref());
    modules.api_request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Stake {
                staking_token: asset,
            },
        },
    )
}

/// swaps all rewards that are not in the target assets and add a reply id to the latest swapmsg
fn swap_rewards_with_reply(
    rewards: Vec<AnsAsset>,
    target_assets: Vec<AssetEntry>,
    modules: &Modules<AutocompounderApp>,
    dex: &String,
    reply_id: u64,
) -> Result<(Vec<CosmosMsg>, SubMsg), AutocompounderError> {
    let mut swap_msgs: Vec<CosmosMsg> = vec![];
    rewards
        .iter()
        .try_for_each(|reward: &AnsAsset| -> StdResult<_> {
            if !target_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = modules.api_request(
                    EXCHANGE,
                    DexExecuteMsg {
                        dex: dex.to_string(),
                        action: DexAction::Swap {
                            offer_asset: reward.clone(),
                            ask_asset: target_assets.get(0).unwrap().clone(),
                            max_spread: None,
                            belief_price: None,
                        },
                    },
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
    let rewards = query_rewards(deps, app, config.pool_data.clone());
    let mut rewards = rewards
        .iter()
        .map(|entry| -> StdResult<AnsAsset> {
            // 2) get the number of LP tokens minted in this transaction
            let tkn = entry.resolve(&deps.querier, &ans_host)?;
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps)?)?;

            Ok(AnsAsset::new(entry.clone(), balance))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;
    rewards = rewards
        .into_iter()
        .filter(|reward| reward.amount != Uint128::zero())
        .collect::<Vec<AnsAsset>>();
    Ok(rewards)
}
