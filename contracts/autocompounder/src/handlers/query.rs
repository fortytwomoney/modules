use crate::contract::AutocompounderApp;
use crate::state::CONFIG;
use cosmwasm_std::{to_binary, Binary, Deps, Env, StdResult};
use forty_two::autocompounder::{AutocompounderQueryMsg, ConfigResponse};

const _DEFAULT_PAGE_SIZE: u8 = 5;
const _MAX_PAGE_SIZE: u8 = 20;

/// Handle queries sent to this app.
pub fn query_handler(
    deps: Deps,
    _env: Env,
    _app: &AutocompounderApp,
    msg: AutocompounderQueryMsg,
) -> StdResult<Binary> {
    match msg {
        AutocompounderQueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

/// Returns the current configuration.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    // crate ConfigResponse from config
    Ok(ConfigResponse {
        staking_contract: config.staking_contract,
        pool_address: config.pool_address,
        pool_data: config.pool_data,
        liquidity_token: config.liquidity_token,
        vault_token: config.vault_token,
        commission_addr: config.commission_addr,
        fees: config.fees,
        bonding_period: config.bonding_period,
        min_unbonding_cooldown: config.min_unbonding_cooldown,
    })
}
