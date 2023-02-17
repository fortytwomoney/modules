use crate::{
    contract::{AutocompounderApp, AutocompounderResult},
    error::AutocompounderError,
};
use abstract_sdk::os::cw_staking::{CwStakingQueryMsg, StakeResponse, CW_STAKING};
use abstract_sdk::{features::Identification, os::objects::AssetEntry, ModuleInterface};
use cosmwasm_std::{Decimal, Deps, Uint128};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw_utils::Duration;
use forty_two::autocompounder::Config;

/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
    unbonding_period: Option<Duration>,
) -> AutocompounderResult<Uint128> {
    let modules = app.modules(deps);

    let query = CwStakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
        unbonding_period,
    };
    let res: StakeResponse = modules.query_api(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn cw20_total_supply(deps: Deps, config: &Config) -> AutocompounderResult<Uint128> {
    let TokenInfoResponse {
        total_supply: vault_tokens_total_supply,
        ..
    } = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    Ok(vault_tokens_total_supply)
}

pub fn check_fee(fee: Decimal) -> Result<(), AutocompounderError> {
    if fee > Decimal::percent(99) {
        return Err(AutocompounderError::InvalidFee {});
    }
    Ok(())
}
