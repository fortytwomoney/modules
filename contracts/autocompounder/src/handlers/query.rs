use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::state::{Claim, CLAIMS, CONFIG, LATEST_UNBONDING, PENDING_CLAIMS};
use abstract_sdk::features::Identification;
use abstract_sdk::os::objects::LpToken;
use abstract_sdk::ApiInterface;
use cosmwasm_std::{to_binary, Binary, Deps, Env, Order, StdResult, Uint128};

use cw_staking::{msg::CwStakingQueryMsg, CW_STAKING};
use cw_storage_plus::Bound;
use cw_utils::Expiration;
use forty_two::autocompounder::{AutocompounderQueryMsg, Config};

const DEFAULT_PAGE_SIZE: u8 = 5;
const MAX_PAGE_SIZE: u8 = 20;

/// Handle queries sent to this app.
pub fn query_handler(
    deps: Deps,
    _env: Env,
    app: &AutocompounderApp,
    msg: AutocompounderQueryMsg,
) -> AutocompounderResult<Binary> {
    match msg {
        AutocompounderQueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        AutocompounderQueryMsg::PendingClaims { address } => {
            Ok(to_binary(&query_pending_claims(deps, address)?)?)
        }
        AutocompounderQueryMsg::Claims { address } => Ok(to_binary(&query_claims(deps, address)?)?),
        AutocompounderQueryMsg::AllClaims { start_after, limit } => {
            Ok(to_binary(&query_all_claims(deps, start_after, limit)?)?)
        }
        AutocompounderQueryMsg::LatestUnbonding {} => {
            Ok(to_binary(&query_latest_unbonding(deps)?)?)
        }
        AutocompounderQueryMsg::TotalLpPosition {} => {
            Ok(to_binary(&query_total_lp_position(app, deps)?)?)
        }
        AutocompounderQueryMsg::Balance { address } => {
            Ok(to_binary(&query_balance(deps, address)?)?)
        }
    }
}

/// Returns the current configuration.
pub fn query_config(deps: Deps) -> AutocompounderResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    // crate ConfigResponse from config
    Ok(config)
}

// write query functions for all State const variables: Claims, PendingClaims, LatestUnbonding

pub fn query_pending_claims(deps: Deps, address: String) -> AutocompounderResult<Uint128> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(Uint128::zero());
    }

    let pending_claims = PENDING_CLAIMS.load(deps.storage, address)?;
    Ok(pending_claims)
}

pub fn query_claims(deps: Deps, address: String) -> AutocompounderResult<Vec<Claim>> {
    let claims = CLAIMS.may_load(deps.storage, address)?.unwrap_or_default();
    Ok(claims)
}

pub fn query_all_claims(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u8>,
) -> AutocompounderResult<Vec<(String, Vec<Claim>)>> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(vec![]);
    }

    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into_bytes()));
    let claims = CLAIMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, claims)| -> StdResult<(String, Vec<Claim>)> { Ok((addr, claims)) })
        }?)
        .collect::<StdResult<Vec<(String, Vec<Claim>)>>>()?;

    Ok(claims)
}

pub fn query_latest_unbonding(deps: Deps) -> AutocompounderResult<Expiration> {
    let latest_unbonding = LATEST_UNBONDING.load(deps.storage)?;
    Ok(latest_unbonding)
}

pub fn query_total_lp_position(
    app: &AutocompounderApp,
    deps: Deps,
) -> AutocompounderResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let apis = app.apis(deps);

    // query staking api for total lp tokens

    let query = CwStakingQueryMsg::Staked {
        provider: config.pool_data.dex.clone(),
        staking_token: LpToken::from(config.pool_data).into(),
        staker_address: app.proxy_address(deps)?.to_string(),
        unbonding_period: config.unbonding_period,
    };
    let res: cw_staking::msg::StakeResponse = apis.query(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn query_balance(deps: Deps, address: String) -> AutocompounderResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let vault_balance: cw20::BalanceResponse = deps
        .querier
        .query_wasm_smart(config.vault_token, &cw20::Cw20QueryMsg::Balance { address })?;
    Ok(vault_balance.balance)
}
