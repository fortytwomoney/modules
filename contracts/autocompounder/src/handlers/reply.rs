use std::arch::aarch64::vqnegb_s8;

use abstract_sdk::base::features::{Identification, AbstractNameService};

use abstract_sdk::os::dex::{DexExecuteMsg, DexAction};
use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken, PoolMetadata};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult, Uint128,
    WasmMsg, BalanceResponse, SubMsg,
};
use cw20::{TokenInfoResponse, Cw20QueryMsg};
use cw20_base::ContractError;
use cw20_base::msg::ExecuteMsg::{Mint};

use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};

use cw20::Cw20QueryMsg::{TokenInfo as Cw20TokenInfo};
use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, SWAPPED_REPLY_ID,
};
use crate::state::{CACHED_USER_ADDR, CONFIG};

use crate::response::MsgInstantiateContractResponse;

// pub fn reply_handler(
//     deps: DepsMut,
//     env: Env,
//     app: AutocompounderApp,
//     reply: Reply,
// ) -> AutocompounderResult {
//     // Logic to execute on example reply
//     match reply.id {
//         INSTANTIATE_REPLY_ID => instantiate_reply(deps, env, app, reply),
//         LP_PROVISION_REPLY_ID => lp_provision_reply(deps, env, app, reply),
//     }
// }

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
    let base_state = app.load_state(deps.storage)?;
    let _proxy = base_state.proxy_address;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let current_vault_supply = vault_token_info.total_supply;

    // 2) get the number of LP tokens minted in this transaction
    let lp_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let new_lp_token_minted = lp_token_info.total_supply;

    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let vault_stake = query_stake(deps.as_ref(), &app, lp_token.clone()); // TODO: THis might need to change to AssetEntry

    // The total value of all LP tokens that are staked by the proxy are equal to the total value of all vault tokens in circulation
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = new_lp_token_minted
        .checked_multiply_ratio(current_vault_supply, vault_stake)
        .unwrap();

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
    let stake_msg = stake_lps(deps, app, "TODO".to_string(), lp_token, new_lp_token_minted);

    Ok(Response::new()
        .add_message(mint_msg)
        .add_message(stake_msg)
        .add_attribute("vault_token_minted", mint_amount))
}

fn query_stake(deps: Deps, app: &AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    // QUERY STAKING MODULE
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_address(deps).unwrap().to_string(),
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    res.amount

    // // // alternative method
    // let modules = app.modules(deps.as_ref());
    // let proxy_stake: StakeResponse = deps.querier.query(
    //     &modules.api_query(CW_STAKING, CwStakingQueryMsg::Stake {
    //         lp_token_name: lp_token_name,               // TODO:: check if this is correct
    //         address: app.proxy_address(deps.as_ref())?.to_string()
    //     })?
    // )?.into();
}

fn stake_lps(
    deps: DepsMut,
    app: AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
) -> CosmosMsg {
    let modules = app.modules(deps.as_ref());

    let msg: CosmosMsg = modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Stake {
                    lp_token: AnsAsset::new(lp_token_name, amount),
                },
            },
        )
        .unwrap();

    return msg;
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
    let mut res = Response::new()
        .add_attribute("action", "lp_claim_reply");
    // 1) claim rewards (this happened in the execution before this reply)
    
    // 2.1) query the rewards
    let rewards = query_rewards(deps.as_ref(), &app, config.pool_data.clone());

    // 2.2) query balance of rewards
    let mut rewards= rewards.iter().map(|entry| -> StdResult<AnsAsset> {
        // 2) get the number of LP tokens minted in this transaction
        let tkn = entry.resolve(&deps.querier, &ans_host)?;
        let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;
        
        Ok(AnsAsset::new(*entry, balance))
    }).collect::<StdResult<Vec<AnsAsset>>>()?;
    // remove zero balances
    rewards = rewards.iter().filter(|reward| reward.amount != Uint128::zero()).map(|a| *a).collect::<Vec<AnsAsset>>();

    // 2) deduct fee from rewards
    let fees = rewards.iter_mut().map(|reward| -> StdResult<AnsAsset>{
        let fee = reward.amount.checked_multiply_ratio(config.fees.performance, Uint128::new(100)).unwrap();
        reward.amount = reward.amount.checked_sub(fee)?;

        Ok(AnsAsset::new(reward.name, fee))
    }).collect::<StdResult<Vec<AnsAsset>>>()?;
    
    // 3) (swap and) Send fees to treasury
    // TODO: swap fees for desired treasury token
    // - if we want to swap, we should just create swap msgs with the last one containing a reply id
    //   and then send the fees to the treasury in the reply
    let fee_transfer_msg = bank.transfer(fees, &config.commission_addr)?;
    res.add_message(fee_transfer_msg)
        .add_attribute("action", "fee_transfer");

    // 3) Swap rewards to token in pool
    let pool_assets = config.pool_data.assets();
    // 3.1) check if asset is not in pool assets

    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // 3.1.1) if all assets are in the pool, we can just provide liquidity
        //  TODO: but we might need to check the length of the rewards. 

        // 3.1.2) provide liquidity
        let lp_msg: CosmosMsg = modules.api_request(
            EXCHANGE,
            DexExecuteMsg {
                dex: config.dex.into(),
                action: DexAction::ProvideLiquidity {
                    assets: rewards,
                    max_spread: None,
                },
            },
        )?;

        Ok(
            Response::new()
                .add_message(fee_transfer_msg)
                .add_message(lp_msg)
                .add_attribute("action", "provide_liquidity"),
        )
    } else {

        
        let mut swap_msgs: Vec<CosmosMsg> = vec![];
        // We could already provide the assets here that are in the pool, but that is rather inefficient as we would have to do it again for all the other assets once swapped.
        rewards.iter().try_for_each(|reward: &AnsAsset| -> StdResult<_> {
            if !pool_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = modules.api_request(EXCHANGE, DexExecuteMsg {
                    dex: config.dex.into(),
                    action: DexAction::Swap { offer_asset: *reward, ask_asset: pool_assets[0], max_spread: None, belief_price: None}
                })?;
                swap_msgs.push(swap_msg);
            }
            Ok(())
        })?;

        // get last swap msg and make it a submsg with reply
        let swap_msg = swap_msgs.pop().unwrap();
        let submsg = SubMsg::reply_on_success(swap_msg, SWAPPED_REPLY_ID);
        
        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        Ok(
            Response::new()
                .add_message(fee_transfer_msg)
                .add_messages(swap_msgs)
                .add_submessage(submsg)
                .add_attribute("action", "swap_rewards")
        )
    }
}

fn query_rewards(deps: Deps, app: &AutocompounderApp, pool_data: PoolMetadata) -> Vec<AssetEntry> {
    // query staking module for which rewards are available
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();
    
    // TODO: Reward query has yet to be implemented
    let query = CwStakingQueryMsg::Rewards {
        address: app.proxy_address(deps).unwrap().to_string(),
        pool_data,
    };
    let res: Vec<AssetEntry> = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    
    res
}


/// Queries the balances of pool assets and provides liquidity to the pool
/// 
/// This function is triggered after the last swap message of the lp_compound_reply
/// and assumes the contract has no other rewards than the ones in the pool assets
pub fn swapped_reply(deps: DepsMut, _env: Env, app: AutocompounderApp, _reply: Reply) -> AutocompounderResult {
    let ans_host = app.ans_host(deps.as_ref())?;
    let modules = app.modules(deps.as_ref());
    let config = CONFIG.load(deps.storage)?;
    
    // 1) query balance of pool tokens
    let mut rewards = config.pool_data.assets().iter().map(|entry| -> StdResult<AnsAsset> {
        let tkn = entry.resolve(&deps.querier, &ans_host)?;
        let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;
        Ok(AnsAsset::new(*entry, balance))

    }).collect::<StdResult<Vec<AnsAsset>>>()?;
    
    // 2) provide liquidity
    let lp_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.dex.into(),
            action: DexAction::ProvideLiquidity {
                assets: rewards,
                max_spread: None,
            },
        },
    )?;

    Ok(
        Response::new()
            .add_message(lp_msg)
            .add_attribute("action", "provide_liquidity")
    )
}