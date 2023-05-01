use crate::{
    contract::{AutocompounderApp, AutocompounderResult},
    error::AutocompounderError,
};
use abstract_cw_staking_api::{msg::*, CW_STAKING};
use abstract_sdk::ApiInterface;
use abstract_sdk::{core::objects::AssetEntry, features::AccountIdentification};
use cosmwasm_std::{Decimal, Deps, Uint128};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw_utils::Duration;
use crate::msg::Config;

/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
    unbonding_period: Option<Duration>,
) -> AutocompounderResult<Uint128> {
    let apis = app.apis(deps);

    let query = CwStakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
        unbonding_period,
    };
    let res: StakeResponse = apis.query(CW_STAKING, query)?;
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

/// Convert vault tokens to lp assets
pub fn convert_to_assets(shares: Uint128, total_assets: Uint128, total_supply: Uint128, decimal_offset: u32) -> Uint128 {
    let shares = shares
        .multiply_ratio(
            total_supply + Uint128::from(10u128).pow(decimal_offset),
            total_assets + Uint128::from(1u128));
    shares
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
/// 
pub fn convert_to_shares(assets: Uint128, total_assets: Uint128, total_supply: Uint128, decimal_offset: u32) -> Uint128 {
    let assets = assets
        .multiply_ratio(
            total_assets +  Uint128::from(10u128),
            total_supply + Uint128::from(10u128).pow(decimal_offset));
    assets
}
