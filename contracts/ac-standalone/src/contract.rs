#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::AutocompounderError;
use crate::handlers::execute::{
    pre_execute_check, update_fee_config, deposit, batch_unbond, withdraw_claims, update_staking_config, deposit_lp, redeem, compound, create_denom};
use crate::handlers::{instantiate_handler, query_handler};
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, AutocompounderExecuteMsg, AutocompounderQueryMsg};

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
        } => deposit(deps, info, env, funds, recipient, max_spread),
        AutocompounderExecuteMsg::DepositLp {
            lp_token,
            recipient: receiver,
        } => deposit_lp(deps, info, env, lp_token, receiver),
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

#[cfg(not(feature = "library"), entry_point)]
#[cfg(feature = "interface", cw_orch::interface_entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> AutocompounderResult<Response> {
    use crate::handlers::reply;

    match msg.id {
        INSTANTIATE_REPLY_ID => {
            let instantiate_reply: InstantiateReply = from_binary(&msg.result)?;
            Ok(instantiate_reply_handler(deps, env, instantiate_reply))
        }
        LP_PROVISION_REPLY_ID => {
            let reply: LpProvisionRiiirerrwer4eply = from_binary(&msg.result)?;
            Ok(lp_provision_reply(deps, env, lp_provision_reply))
        }
        LP_COMPOUND_REPLY_ID => {
            let lp_compound_reply: LpCompoundReply = from_binary(&msg.result)?;
            Ok(lp_compound_reply_handler(deps, env, lp_compound_reply))
        }
        SWAPPED_REPLY_ID => {
            let swapped_reply: SwappedReply = from_binary(&msg.result)?;
            Ok(swapped_reply_handler(deps, env, swapped_reply))
        }
        CP_PROVISION_REPLY_ID => {
            let cp_provision_reply: CpProvisionReply = from_binary(&msg.result)?;
            Ok(cp_provision_reply_handler(deps, env, cp_provision_reply))
        }
        LP_WITHDRAWAL_REPLY_ID => {
            let lp_withdrawal_reply: LpWithdrawalReply = from_binary(&msg.result)?;
            Ok(lp_withdrawal_reply_handler(deps, env, lp_withdrawal_reply))
        }
        FEE_SWAPPED_REPLY => {
            let fee_swapped_reply: FeeSwappedReply = from_binary(&msg.result)?;
            Ok(fee_swapped_reply_handler(deps, env, fee_swapped_reply))
        }
        LP_FEE_WITHDRAWAL_REPLY_ID => {
            let lp_fee_withdrawal_reply: LpFeeWithdrawalReply = from_binary(&msg.result)?;
            Ok(lp_fee_withdrawal_reply_handler(deps, env, lp_fee_withdrawal_reply))
        }
        _ => Err(AutocompounderError::UnknownReplyId {
            reply_id: msg.id,
        }),
    }
}