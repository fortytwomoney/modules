#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Reply, Attribute};
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use reply::{
    compound_lp_provision_reply, instantiate_reply, lp_compound_reply, lp_provision_reply,
    lp_withdrawal_reply,
};

use crate::handlers::{reply, execute_handler};

use crate::error::AutocompounderError;
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

pub type AutocompounderResult<T = Response, E = AutocompounderError> = Result<T, E>;

#[cfg(feature = "interface")]
use cw_orch::interface_entry_point;

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, AutocompounderError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    instantiate_handler(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, AutocompounderError> {
    execute_handler(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
#[cfg_attr(feature = "interface", cw_orch::interface_entry_point)] // cw-orch automatic
pub fn query(deps: Deps, env: Env, msg: AutocompounderQueryMsg) -> AutocompounderResult<Binary> {
    query_handler(deps, env, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> AutocompounderResult {
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

