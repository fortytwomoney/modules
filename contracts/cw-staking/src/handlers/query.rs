use crate::contract::CwStakingApi;
use cosmwasm_std::{Binary, Deps, Env, StdError, StdResult};
use forty_two::cw_staking::CwStakingQueryMsg;

pub fn query_handler(
    _deps: Deps,
    _env: Env,
    _app: &CwStakingApi,
    _msg: CwStakingQueryMsg,
) -> StdResult<Binary> {
    Err(StdError::generic_err("Unknown query"))
}
