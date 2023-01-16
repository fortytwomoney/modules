use abstract_sdk::{base::features::Identification, os::objects::AssetEntry, ModuleInterface};
use cosmwasm_std::{Decimal, Deps, StdResult, Uint128};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use forty_two::autocompounder::{Config, FeeConfig};
use forty_two::cw_staking::{CwStakingQueryMsg, StakeResponse, CW_STAKING};

use crate::{contract::AutocompounderApp, error::AutocompounderError};

/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
) -> StdResult<Uint128> {
    let modules = app.modules(deps);

    let query = CwStakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
    };
    let res: StakeResponse = modules.query_api(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn cw20_total_supply(deps: Deps, config: &Config) -> StdResult<Uint128> {
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    let vault_tokens_total_supply = vault_token_info.total_supply;
    Ok(vault_tokens_total_supply)
}

pub fn check_fee(fee: Decimal) -> Result<(), AutocompounderError> {
    if fee > Decimal::percent(99) {
        return Err(AutocompounderError::InvalidFee {});
    }
    Ok(())
}
