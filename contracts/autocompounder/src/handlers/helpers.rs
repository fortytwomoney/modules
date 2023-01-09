use abstract_sdk::{os::objects::AssetEntry, ModuleInterface, base::features::Identification};
use cosmwasm_std::{Deps, Uint128, StdResult};
use cw20::{TokenInfoResponse, Cw20QueryMsg};
use forty_two::cw_staking::{CW_STAKING, CwStakingQueryMsg, StakeResponse};

use crate::{contract::AutocompounderApp, state::Config};


/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
) -> StdResult<Uint128> {
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING)?;

    let query = CwStakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query)?;
    Ok(res.amount)
}

pub fn cw20_total_supply(deps: Deps, config: &Config) -> StdResult<Uint128> {
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    let vault_tokens_total_supply = vault_token_info.total_supply;
    Ok(vault_tokens_total_supply)
}
