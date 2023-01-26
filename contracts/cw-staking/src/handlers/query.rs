use crate::{
    contract::CwStakingApi,
    providers::resolver::{self, is_over_ibc},
};
use abstract_sdk::base::features::AbstractNameService;
use cosmwasm_std::{to_binary, Binary, Deps, Env, StdError, StdResult};
use forty_two::cw_staking::CwStakingQueryMsg;

pub fn query_handler(
    deps: Deps,
    _env: Env,
    app: &CwStakingApi,
    msg: CwStakingQueryMsg,
) -> StdResult<Binary> {
    let name_service = app.name_service(deps);
    let ans_host = name_service.host();

    match msg {
        CwStakingQueryMsg::Info {
            provider,
            staking_token,
        } => {
            // if provider is on an app-chain, error
            if is_over_ibc(&provider)? {
                Err(StdError::generic_err("IBC queries not supported."))
            } else {
                // the query can be executed on the local chain
                let mut provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                provider.fetch_data(deps, ans_host, staking_token)?;
                to_binary(&provider.query_info(&deps.querier)?)
            }
        }
        CwStakingQueryMsg::Staked {
            provider,
            staking_token,
            staker_address,
            unbonding_period,
        } => {
            // if provider is on an app-chain, error
            if is_over_ibc(&provider)? {
                Err(StdError::generic_err("IBC queries not supported."))
            } else {
                // the query can be executed on the local chain
                let mut provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                provider.fetch_data(deps, ans_host, staking_token)?;
                to_binary(&provider.query_staked(
                    &deps.querier,
                    deps.api.addr_validate(&staker_address)?,
                    unbonding_period,
                )?)
            }
        }
        CwStakingQueryMsg::Unbonding {
            provider,
            staking_token,
            staker_address,
        } => {
            // if provider is on an app-chain, error
            if is_over_ibc(&provider)? {
                Err(StdError::generic_err("IBC queries not supported."))
            } else {
                // the query can be executed on the local chain
                let mut provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                provider.fetch_data(deps, ans_host, staking_token)?;
                to_binary(
                    &provider
                        .query_unbonding(&deps.querier, deps.api.addr_validate(&staker_address)?)?,
                )
            }
        }
        CwStakingQueryMsg::RewardTokens {
            provider,
            staking_token,
        } => {
            // if provider is on an app-chain, error
            if is_over_ibc(&provider)? {
                Err(StdError::generic_err("IBC queries not supported."))
            } else {
                // the query can be executed on the local chain
                let mut provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                provider.fetch_data(deps, ans_host, staking_token)?;
                to_binary(&provider.query_reward_tokens(&deps.querier)?)
            }
        }
    }
}
