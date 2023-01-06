use crate::{contract::CwStakingApi, providers::resolver};
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
            let provider_id = resolver::resolve_provider_by_name(&provider).unwrap();
            // if provider is on an app-chain, error
            if provider_id.over_ibc() {
                return Err(StdError::generic_err("IBC queries not supported."));
            } else {
                // the query can be executed on the local chain
                let provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                let staking_address =
                    provider.staking_contract_address(deps, ans_host, &staking_token)?;
                to_binary(&provider.query_info(&deps.querier, staking_address)?)
            }
        }
        CwStakingQueryMsg::Staked {
            provider,
            staking_token,
            staker_address,
        } => {
            let provider_id = resolver::resolve_provider_by_name(&provider).unwrap();
            // if provider is on an app-chain, error
            if provider_id.over_ibc() {
                return Err(StdError::generic_err("IBC queries not supported."));
            } else {
                // the query can be executed on the local chain
                let provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                let staking_address =
                    provider.staking_contract_address(deps, ans_host, &staking_token)?;
                to_binary(&provider.query_staked(
                    &deps.querier,
                    staking_address,
                    deps.api.addr_validate(&staker_address)?,
                )?)
            }
        }
        CwStakingQueryMsg::Unbonding {
            provider,
            staking_token,
            staker_address,
        } => {
            let provider_id = resolver::resolve_provider_by_name(&provider).unwrap();
            // if provider is on an app-chain, error
            if provider_id.over_ibc() {
                return Err(StdError::generic_err("IBC queries not supported."));
            } else {
                // the query can be executed on the local chain
                let provider = resolver::resolve_local_provider(&provider)
                    .map_err(|e| StdError::generic_err(e.to_string()))?;
                let staking_address =
                    provider.staking_contract_address(deps, ans_host, &staking_token)?;
                to_binary(&provider.query_unbonding(
                    &deps.querier,
                    staking_address,
                    deps.api.addr_validate(&staker_address)?,
                )?)
            }
        }
    }
}
