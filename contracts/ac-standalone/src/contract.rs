#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Reply, Attribute};
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use reply::{
    compound_lp_provision_reply, instantiate_reply, lp_compound_reply, lp_provision_reply,
    lp_withdrawal_reply,
};

use crate::handlers::reply;

use crate::error::AutocompounderError;
use crate::handlers::execute::{
    batch_unbond, compound, create_denom, deposit, deposit_lp, pre_execute_check, redeem,
    update_fee_config, update_staking_config, withdraw_claims,
};
use crate::handlers::{instantiate_handler, query_handler};
use crate::msg::{AutocompounderExecuteMsg, AutocompounderQueryMsg, ExecuteMsg, InstantiateMsg, AUTOCOMPOUNDER};

// version info for migration info
pub const CONTRACT_NAME: &str = "crates.io:cw1-general";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const INSTANTIATE_REPLY_ID: u64 = 0u64;
pub const LP_PROVISION_REPLY_ID: u64 = 1u64;
pub const LP_COMPOUND_REPLY_ID: u64 = 2u64;
pub const SWAPPED_REPLY_ID: u64 = 3u64;
pub const CP_PROVISION_REPLY_ID: u64 = 4u64;
pub const LP_WITHDRAWAL_REPLY_ID: u64 = 5u64;
pub const FEE_SWAPPED_REPLY: u64 = 6u64;
pub const LP_FEE_WITHDRAWAL_REPLY_ID: u64 = 7u64;

pub type AutocompounderResult<T = Response> = Result<T, AutocompounderError>;

pub fn autocompounder_response(action: str, attributes: Vec<(&str, &str)>) -> Response {
    Ok(Response::new()
        .add_attributes(
            vec![
                ("contract", AUTOCOMPOUNDER)
                ("action", action),
            ]
        )
        .add_attributes(attributes)
    )
}

#[cfg(feature = "interface")]
use cw_orch::interface_entry_point;

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(instantiate_handler(deps, env, info, msg))
    // Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, AutocompounderError> {
    pre_execute_check(&msg, deps.as_ref())?;

    match msg {
        AutocompounderExecuteMsg::UpdateFeeConfig {
            performance,
            withdrawal,
            deposit,
            fee_collector_addr,
        } => update_fee_config(
            deps,
            info,
            performance,
            withdrawal,
            deposit,
            fee_collector_addr,
        ),
        AutocompounderExecuteMsg::Deposit {
            funds,
            recipient,
            max_spread,
        } => deposit(deps, info, env, recipient, max_spread),
        AutocompounderExecuteMsg::DepositLp {
            lp_token,
            recipient: receiver,
        } => deposit_lp(deps, info, env, receiver),
        AutocompounderExecuteMsg::Redeem { amount, recipient } => {
            redeem(deps, env, info.sender, amount, recipient)
        }
        AutocompounderExecuteMsg::Withdraw {} => withdraw_claims(deps, env, info.sender),
        AutocompounderExecuteMsg::BatchUnbond { start_after, limit } => {
            batch_unbond(deps, env, start_after, limit)
        }
        AutocompounderExecuteMsg::Compound {} => compound(deps),
        AutocompounderExecuteMsg::UpdateStakingConfig {
            preferred_bonding_period,
        } => update_staking_config(deps, info, preferred_bonding_period),
        AutocompounderExecuteMsg::CreateDenom {} => create_denom(deps, info, &env),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn query(deps: Deps, env: Env, msg: AutocompounderQueryMsg) -> StdResult<Binary> {
    query_handler(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> AutocompounderResult<Response> {
    match msg.id {
        INSTANTIATE_REPLY_ID => instantiate_reply(deps, env, msg),
        LP_PROVISION_REPLY_ID => lp_provision_reply(deps, env, msg),
        LP_COMPOUND_REPLY_ID => compound_lp_provision_reply(deps, env, msg),
        SWAPPED_REPLY_ID => lp_compound_reply(deps, env, msg),
        CP_PROVISION_REPLY_ID => lp_compound_reply(deps, env, msg),
        LP_WITHDRAWAL_REPLY_ID => lp_withdrawal_reply(deps, env, msg),
        FEE_SWAPPED_REPLY => lp_compound_reply(deps, env, msg),
        LP_FEE_WITHDRAWAL_REPLY_ID => lp_withdrawal_reply(deps, env, msg),
        _ => Err(AutocompounderError::Std(
            cosmwasm_std::StdError::GenericErr {
                msg: format!("ReplyId {} not found", msg.id),
            },
        )),
    }
}
