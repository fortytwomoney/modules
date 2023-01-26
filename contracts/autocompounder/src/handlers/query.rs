use crate::contract::AutocompounderApp;
use crate::state::{CONFIG, CLAIMS, PENDING_CLAIMS, LATEST_UNBONDING, Claim};
use abstract_sdk::os::objects::LpToken;
use abstract_sdk::ModuleInterface;
use abstract_sdk::base::features::Identification;
use cosmwasm_std::{to_binary, Binary, Deps, Env, StdResult, Order, Uint128};

use cw_storage_plus::Bound;
use cw_utils::Expiration;
use forty_two::autocompounder::{AutocompounderQueryMsg, Config};
use forty_two::cw_staking::{CwStakingQueryMsg, CW_STAKING};

const DEFAULT_PAGE_SIZE: u8 = 5;
const MAX_PAGE_SIZE: u8 = 20;

/// Handle queries sent to this app.
pub fn query_handler(
    deps: Deps,
    _env: Env,
    app: &AutocompounderApp,
    msg: AutocompounderQueryMsg,
) -> StdResult<Binary> {
    match msg {
        AutocompounderQueryMsg::Config {} => to_binary(&query_config(deps)?),
        AutocompounderQueryMsg::PendingClaims { address } => to_binary(&query_pending_claims(deps, address)?),
        AutocompounderQueryMsg::Claims { address } => to_binary(&query_claims(deps, address)?),
        AutocompounderQueryMsg::AllClaims { start_after, limit } => to_binary(&query_all_claims(deps, start_after, limit)?),
        AutocompounderQueryMsg::LatestUnbonding {} => to_binary(&query_latest_unbonding(deps)?),
        AutocompounderQueryMsg::TotalLpPosition {  } => to_binary(&query_total_lp_position(app, deps)?),
        AutocompounderQueryMsg::Balance { address } => to_binary(&query_balance(app, deps, address)?),
    }
}

/// Returns the current configuration.
pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    // crate ConfigResponse from config
    Ok(config)
}

// write query functions for all State const variables: Claims, PendingClaims, LatestUnbonding

pub fn query_pending_claims(deps: Deps, address: String) -> StdResult<Uint128> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(Uint128::zero());
    }

    let pending_claims = PENDING_CLAIMS.load(deps.storage, address)?;
    Ok(pending_claims)
}

pub fn query_claims(deps: Deps, address: String) -> StdResult<Vec<Claim>> {
    let claims = CLAIMS.load(deps.storage, address)?;
    Ok(claims)
}

pub fn query_all_claims(deps: Deps, start_after: Option<String>, limit: Option<u8>) -> StdResult<Vec<(String,Vec<Claim>)>> {
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
            item.map(|(addr, claims)| -> StdResult<(String, Vec<Claim>)>{
                Ok((addr, claims))
        })
        }?)
        .collect::<StdResult<Vec<(String, Vec<Claim>)>>>()?;
    
    Ok(claims)
}

pub fn query_latest_unbonding(deps: Deps) -> StdResult<Expiration> {
    let latest_unbonding = LATEST_UNBONDING.load(deps.storage)?;
    Ok(latest_unbonding)
}


pub fn query_total_lp_position(app: &AutocompounderApp, deps: Deps) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let modules = app.modules(deps);

    // query staking api for total lp tokens

    let query = CwStakingQueryMsg::Staked { 
        provider: config.pool_data.dex.clone(), 
        staking_token: LpToken::from(config.pool_data).into() , 
        staker_address: app.proxy_address(deps)?.to_string(),
        unbonding_period: config.unbonding_period};
    let res: forty_two::cw_staking::StakeResponse = modules.query_api(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn query_balance(app: &AutocompounderApp, deps: Deps, address: String) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let vault_balance: cw20::BalanceResponse = deps
    .querier
    .query_wasm_smart(config.vault_token.clone(), &cw20::Cw20QueryMsg::Balance { address })?;
    Ok(vault_balance.balance)
}