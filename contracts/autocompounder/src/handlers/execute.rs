use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order,
    QuerierWrapper, QueryRequest, ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
    WasmQuery,
};
use cw20::{Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};

use cw_asset::AssetList;
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};
use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};
use schemars::_private::NoSerialize;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_PROVISION_REPLY_ID, LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::state::{Claim, CACHED_USER_ADDR, CLAIMS, CONFIG, PENDING_CLAIMS};

/// Handle the `AutocompounderExecuteMsg`s sent to this app.
pub fn execute_handler(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderExecuteMsg,
) -> AutocompounderResult {
    match msg {
        AutocompounderExecuteMsg::UpdateFeeConfig {
            performance,
            withdrawal,
            deposit,
        } => update_fee_config(deps, info, app, performance, withdrawal, deposit),
        AutocompounderExecuteMsg::Deposit { funds } => deposit(deps, info, _env, app, funds),
        AutocompounderExecuteMsg::BatchUnbond {} => batch_unbond(deps, info, _env, app),
        _ => Err(AutocompounderError::ExceededMaxCount {}),
        AutocompounderExecuteMsg::Withdraw {} => todo!(),
        AutocompounderExecuteMsg::Compound {} => todo!(),
    }
}

/// Update the application configuration.
pub fn update_fee_config(
    deps: DepsMut,
    msg_info: MessageInfo,
    dapp: AutocompounderApp,
    _fee: Option<Uint128>,
    _withdrawal: Option<Uint128>,
    _deposit: Option<Uint128>,
) -> AutocompounderResult {
    dapp.admin.assert_admin(deps.as_ref(), &msg_info.sender)?;

    unimplemented!()
}

// im assuming that this is the function that will be called when the user wants to pool AND stake their funds
pub fn deposit(
    deps: DepsMut,
    msg_info: MessageInfo,
    _env: Env,
    app: AutocompounderApp,
    funds: Vec<AnsAsset>,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    let config = CONFIG.load(deps.storage)?;
    let _staking_address = config.staking_contract;
    let ans_host = app.ans_host(deps.as_ref())?;

    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits
        .deduct_many(&msg_info.funds.clone().into())?
        .purge();

    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>, _> = claimed_deposits
        .into_iter()
        .map(|asset| {
            // transfer cw20 tokens to the OS
            // will fail if allowance is not set or if some other assets are sent
            asset.transfer_from_msg(&msg_info.sender, app.proxy_address(deps.as_ref())?)
        })
        .collect();

    // transfer received coins to the bank contract
    let bank = app.bank(deps.as_ref());
    bank.deposit_coins(msg_info.funds)?;

    let modules = app.modules(deps.as_ref());
    let swap_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.dex.into(),
            action: DexAction::ProvideLiquidity {
                assets: funds,
                max_spread: None,
            },
        },
    )?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: swap_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &msg_info.sender)?;
    Ok(Response::new()
        .add_messages(cw_20_transfer_msgs_res?)
        .add_submessage(sub_msg)
        .add_attribute("action", "4T2/AC/Deposit"))
}

pub fn batch_unbond(
    deps: DepsMut,
    msg_info: MessageInfo,
    env: Env,
    dapp: AutocompounderApp,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let pending_claims: StdResult<Vec<_>> = PENDING_CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .collect();
    let mut total_lp_amount_to_unbond = Uint128::from(0u128);
    let mut total_vault_tokens_to_burn = Uint128::from(0u128);

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    let vault_tokens_total_supply = vault_token_info.total_supply;

    // 2) get total amount of LP tokens staked in vault
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let total_lp_tokens_staked_in_vault = query_stake(deps.as_ref(), &dapp, lp_token.clone());

    // 3) calculate lp tokens amount to withdraw per each user
    for pending_claim in pending_claims? {
        let user_address = pending_claim.0;
        let user_amount_of_vault_tokens_to_be_burned = pending_claim.1;

        let user_lp_tokens_withdraw_amount = Decimal::from_ratio(
            user_amount_of_vault_tokens_to_be_burned,
            vault_tokens_total_supply,
        ) * total_lp_tokens_staked_in_vault;

        total_lp_amount_to_unbond = total_lp_amount_to_unbond
            .checked_add(user_lp_tokens_withdraw_amount)
            .unwrap();

        total_vault_tokens_to_burn = total_vault_tokens_to_burn
            .checked_add(user_amount_of_vault_tokens_to_be_burned)
            .unwrap();

        let new_claim = Claim {
            unbonding_timestamp: env.block.time,
            amount_of_vault_tokens_to_burn: user_amount_of_vault_tokens_to_be_burned,
            amount_of_lp_tokens_to_unbond: user_lp_tokens_withdraw_amount,
        };

        if let Some(mut existent_claims) = CLAIMS.may_load(deps.storage, user_address.clone())? {
            existent_claims.push(new_claim);
            CLAIMS.save(deps.storage, user_address, &existent_claims)?;
        } else {
            CLAIMS.save(deps.storage, user_address, &vec![new_claim])?;
        }
    }

    // clear pending claims
    PENDING_CLAIMS.clear(deps.storage);

    let unstake_msg =
        unstake_lp_tokens(deps, dapp, config.dex, lp_token, total_lp_amount_to_unbond);

    let burn_msg = get_burn_msg(&config.vault_token, total_vault_tokens_to_burn)?;

    Ok(Response::new()
        .add_messages(vec![unstake_msg, burn_msg])
        .add_attribute("action", "4T2/AC/UnbondBatch"))
}

/// Handles receiving CW20 messages
pub fn receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    dapp: AutocompounderApp,
    msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    // Withdraw fn can only be called by liquidity token
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.vault_token {
        return Err(AutocompounderError::SenderIsNotVaultToken {});
    }

    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, dapp, msg.sender, msg.amount),
    }
}

fn redeem(
    deps: DepsMut,
    env: Env,
    dapp: AutocompounderApp,
    sender: String,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &sender)?;

    // let mut messages = vec![];

    if let Some(pending_claim) = PENDING_CLAIMS.may_load(deps.storage, sender.to_string())? {
        let new_pending_claim = pending_claim
            .checked_add(amount_of_vault_tokens_to_be_burned)
            .unwrap();
        PENDING_CLAIMS.save(deps.storage, sender.to_string(), &new_pending_claim)?;
    } else {
        PENDING_CLAIMS.save(
            deps.storage,
            sender.to_string(),
            &amount_of_vault_tokens_to_be_burned,
        )?;
    }

    // // check if claim exists for this user
    // if let Some(existent_user_claim) = CLAIMS.may_load(deps.storage, sender.to_string())? {
    //     let time_diff = env
    //         .block
    //         .time
    //         .minus_nanos(existent_user_claim.unbonding_timestamp.nanos());
    //     // Compares time difference between start of unbonding period and current time against pool bonding period
    //     // if time diff is greater than bonding period that means tokens are ready to be withdrawn
    //     if time_diff >= config.bonding_period {
    //         // Remove claim from user
    //         CLAIMS.remove(deps.storage, sender.to_string());
    //         // 4) claim lp tokens
    //         let claim_unbonded_lps_msg =
    //             claim_lps(deps.as_ref(), &dapp, config.dex.clone(), lp_token.clone());

    //         messages.push(claim_unbonded_lps_msg);

    //         let modules = dapp.modules(deps.as_ref());

    //         let withdraw_liquidity_msg: CosmosMsg = modules.api_request(
    //             EXCHANGE,
    //             DexExecuteMsg {
    //                 dex: config.dex.into(),
    //                 action: DexAction::WithdrawLiquidity {
    //                     lp_token: lp_token.clone(),
    //                     amount: lp_tokens_withdraw_amount,
    //                 },
    //             },
    //         )?;

    //         let withdraw_liquidity_sub_msg = SubMsg {
    //             id: LP_WITHDRAWAL_REPLY_ID,
    //             msg: withdraw_liquidity_msg,
    //             gas_limit: None,
    //             reply_on: ReplyOn::Success,
    //         };

    //         let vault_token_burn_msg =
    //             get_burn_msg(&config.vault_token, amount_of_vault_tokens_to_be_burned)?;
    //         messages.push(vault_token_burn_msg);

    //         Ok(Response::new()
    //             .add_messages(messages)
    //             .add_submessage(withdraw_liquidity_sub_msg))
    //     } else {
    //         // unbonding period still not completed
    //         return Err(AutocompounderError::TokensStillBeingUnbonded {});
    //     }
    // } else {
    //     // Start unbonding process
    //     let claim = Claim {
    //         unbonding_timestamp: env.block.time,
    //         amount_of_vault_tokens_to_burn: amount_of_vault_tokens_to_be_burned,
    //         amount_of_lp_tokens_to_unbond: lp_tokens_withdraw_amount,
    //     };

    //     CLAIMS.save(deps.storage, sender.to_string(), &claim)?;

    //     // TODO: send unbond message
    //     Ok(Response::new())
    // }
    Ok(Response::new().add_attribute("action", "4T2/AC/Register_pre_claim"))
}

// fn get_token_amount(
//     deps: DepsMut,
//     env: Env,
//     sender: String,
//     amount: Uint128,
// ) -> AutocompounderResult {
//     let config = CONFIG.load(deps.storage)?;
// }

fn get_token_info(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<TokenInfoResponse> {
    let token_info: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(token_info)
}

pub fn query_stake(deps: Deps, app: &AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_address(deps).unwrap().to_string(),
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    res.amount
}

fn claim_lps(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let modules = app.modules(deps);

    let msg: CosmosMsg = modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Claim { lp_token_name },
            },
        )
        .unwrap();

    return msg;
}

fn get_burn_msg(contract: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::Burn { amount };
    Ok(WasmMsg::Execute {
        contract_addr: contract.to_string(),
        msg: to_binary(&msg)?,
        funds: vec![],
    }
    .into())
}

fn unstake_lp_tokens(
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
                action: CwStakingAction::Unstake {
                    lp_token: AnsAsset::new(lp_token_name, amount),
                },
            },
        )
        .unwrap();

    return msg;
}
