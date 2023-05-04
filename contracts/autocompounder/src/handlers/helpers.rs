use crate::msg::Config;
use crate::{
    contract::{AutocompounderApp, AutocompounderResult},
    error::AutocompounderError,
};
use abstract_core::objects::AnsAsset;
use abstract_cw_staking_api::{msg::*, CW_STAKING};
use abstract_sdk::{core::objects::AssetEntry, features::AccountIdentification};
use abstract_sdk::{AbstractSdkResult, ApiInterface};
use cosmwasm_std::{wasm_execute, Addr, CosmosMsg, Decimal, Deps, Uint128};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;
use cw_utils::Duration;

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

pub fn mint_vault_tokens(
    config: &Config,
    user_address: Addr,
    mint_amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    let mint_msg = wasm_execute(
        config.vault_token.to_string(),
        &Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        },
        vec![],
    )?
    .into();
    Ok(mint_msg)
}

pub fn stake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    asset: AnsAsset,
    unbonding_period: Option<Duration>,
) -> AbstractSdkResult<CosmosMsg> {
    let apis = app.apis(deps);
    apis.request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Stake {
                staking_token: asset,
                unbonding_period,
            },
        },
    )
}

/// Convert vault tokens to lp assets
pub fn convert_to_assets(
    shares: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    let shares = shares.multiply_ratio(
        total_assets + Uint128::from(1u128),
        total_supply + Uint128::from(10u128).pow(decimal_offset),
    );
    shares
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
pub fn convert_to_shares(
    assets: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    let assets = assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(decimal_offset),
        total_assets + Uint128::from(1u128),
    );
    assets
}
