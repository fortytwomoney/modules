use abstract_sdk::base::features::{AbstractNameService, Identification};

use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};
use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken, PoolMetadata};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult, SubMsg,
    Uint128, WasmMsg,
};
use cw20::TokenInfoResponse;
use cw20_base::msg::ExecuteMsg::Mint;

use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};

use cw20::Cw20QueryMsg::TokenInfo as Cw20TokenInfo;
use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID, SWAPPED_REPLY_ID,
};
use crate::state::{CACHED_USER_ADDR, CONFIG};

use crate::response::MsgInstantiateContractResponse;

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
    env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let _ans_host = app.ans_host(deps.as_ref())?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let current_vault_supply = vault_token_info.total_supply;

    // 2) Retrieve the number of LP tokens minted/staked.
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let staked_lp = query_stake(deps.as_ref(), &app, env, lp_token.clone(), config.dex.clone())?;
    let cw20::BalanceResponse {
        balance: received_lp,
    } = deps.querier.query_wasm_smart(
        config.vault_token.clone(),
        &cw20::Cw20QueryMsg::Balance {
            address: proxy_address.to_string(),
        },
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
    let stake_msg = stake_lps(deps, app, config.dex, lp_token, received_lp);

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

fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    env: Env,
    lp_token_name: AssetEntry,
    dex: String,
) -> StdResult<Uint128> {
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Staked {
        staker_address: env.contract.address.to_string(),
        provider: dex,
        staking_token: lp_token_name,
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    Ok(res.amount)
}

// TODO: move to cw_staking SDK
fn stake_lps(
    deps: DepsMut,
    app: AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
) -> CosmosMsg {
    let modules = app.modules(deps.as_ref());
    modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Stake {
                    staking_token: AnsAsset::new(lp_token_name, amount),
                },
            },
        )
        .unwrap()
}

pub fn lp_compound_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let ans_host = app.ans_host(deps.as_ref())?;
    let bank = app.bank(deps.as_ref());
    let modules = app.modules(deps.as_ref());

    let config = CONFIG.load(deps.storage)?;
    let base_state = app.load_state(deps.storage)?;
    let _proxy = base_state.proxy_address;
    // 1) claim rewards (this happened in the execution before this reply)

    // 2.1) query the rewards
    let rewards = query_rewards(deps.as_ref(), &app, config.pool_data.clone());

    // 2.2) query balance of rewards
    // TODO: use bank.balances query
    let mut rewards = rewards
        .iter()
        .map(|entry| -> StdResult<AnsAsset> {
            // 2) get the number of LP tokens minted in this transaction
            let tkn = entry.resolve(&deps.querier, &ans_host)?;
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;

            Ok(AnsAsset::new(entry.clone(), balance))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;
    // remove zero balances
    rewards = rewards
        .into_iter()
        .filter(|reward| reward.amount != Uint128::zero())
        .collect::<Vec<AnsAsset>>();

    // 2) deduct fee from rewards
    let fees = rewards
        .iter_mut()
        .map(|reward| -> StdResult<AnsAsset> {
            let fee = reward
                .amount
                .checked_multiply_ratio(config.fees.performance, Uint128::new(100))
                .unwrap();
            reward.amount = reward.amount.checked_sub(fee)?;

            Ok(AnsAsset::new(reward.name.clone(), fee))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;

    // 3) (swap and) Send fees to treasury
    // TODO: swap fees for desired treasury token
    // - if we want to swap, we should just create swap msgs with the last one containing a reply id
    //   and then send the fees to the treasury in the reply
    let fee_transfer_msg = bank.transfer(fees, &config.commission_addr)?;

    // 3) Swap rewards to token in pool
    let pool_assets = config.pool_data.assets;
    // 3.1) check if asset is not in pool assets

    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // 3.1.1) if all assets are in the pool, we can just provide liquidity
        //  TODO: but we might need to check the length of the rewards.

        // 3.1.2) provide liquidity
        let lp_msg: CosmosMsg = modules.api_request(
            EXCHANGE,
            DexExecuteMsg {
                dex: config.dex,
                action: DexAction::ProvideLiquidity {
                    assets: rewards,
                    max_spread: None,
                },
            },
        )?;

        let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);
        Ok(Response::new()
            .add_message(fee_transfer_msg)
            .add_submessage(submsg)
            .add_attribute("action", "provide_liquidity"))
    } else {
        let mut swap_msgs: Vec<CosmosMsg> = vec![];
        // We could already provide the assets here that are in the pool, but that is rather inefficient as we would have to do it again for all the other assets once swapped.
        rewards
            .iter()
            .try_for_each(|reward: &AnsAsset| -> StdResult<_> {
                if !pool_assets.contains(&reward.name) {
                    // 3.2) swap to asset in pool
                    let swap_msg = modules.api_request(
                        EXCHANGE,
                        DexExecuteMsg {
                            dex: config.dex.clone(),
                            action: DexAction::Swap {
                                offer_asset: reward.clone(),
                                ask_asset: pool_assets.get(0).unwrap().clone(),
                                max_spread: None,
                                belief_price: None,
                            },
                        },
                    )?;
                    swap_msgs.push(swap_msg);
                }
                Ok(())
            })?;

        // get last swap msg and make it a submsg with reply
        // could panic if rewards is empty
        let swap_msg = swap_msgs.pop().unwrap();
        let submsg = SubMsg::reply_on_success(swap_msg, SWAPPED_REPLY_ID);

        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        Ok(Response::new()
            .add_message(fee_transfer_msg)
            .add_messages(swap_msgs)
            .add_submessage(submsg)
            .add_attribute("action", "swap_rewards"))
    }

    // TODO: stake lp tokens
}

fn query_rewards(deps: Deps, app: &AutocompounderApp, _pool_data: PoolMetadata) -> Vec<AssetEntry> {
    // query staking module for which rewards are available
    let modules = app.modules(deps);
    let _staking_mod = modules.module_address(CW_STAKING).unwrap();

    // TODO: Reward query has yet to be implemented
    // let query = CwStakingQueryMsg::Rewards {
    //     address: app.proxy_address(deps).unwrap().to_string(),
    //     pool_data,
    // };
    // let res: Vec<AssetEntry> = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    let res: Vec<AssetEntry> = vec![];

    res
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
            dex: config.dex,
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

    // 1) query balance of lp tokens
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let lp_balance = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy)?;

    // 2) stake lp tokens
    let stake_msg = stake_lps(deps, app, "TODO".to_string(), lp_token, lp_balance);

    Ok(Response::new()
        .add_message(stake_msg)
        .add_attribute("action", "stake"))
}
