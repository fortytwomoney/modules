use std::borrow::BorrowMut;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{Config, FeeConfig, CONFIG, FEE_CONFIG};
use abstract_core::objects::{AssetEntry, PoolAddress, PoolMetadata};
use cosmwasm_std::{DepsMut, Env, Response, StdError, from_slice, Addr, Decimal};
use cw_asset::AssetInfo;
use cw_utils::Duration;

/// Unused for now but provided here as an example
/// Contract version is migrated automatically
pub fn migrate_handler(
    mut deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    _msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    update_config_v0_4_3_to_v0_4_4(deps.borrow_mut())?;

    update_fee_config_v0_4_3_to_v0_4_4(deps.borrow_mut())?;

    Ok(Response::default())
}

/// Vault fee structure
#[cosmwasm_schema::cw_serde]
pub struct OldFeeConfig {
    pub performance: Decimal,
    pub deposit: Decimal,
    pub withdrawal: Decimal,
    pub fee_asset: AssetEntry,
    /// Address that receives the fee commissions
    pub fee_collector_addr: Addr,
}

#[cosmwasm_schema::cw_serde]
pub struct OldConfig {
    /// Address of the staking contract
    pub staking_contract: Addr,
    /// Pool address (number or Address)
    pub pool_address: PoolAddress,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Resolved pool assets
    pub pool_assets: Vec<AssetInfo>,
    /// Address of the LP token contract
    pub liquidity_token: Addr,
    /// Vault token
    pub vault_token: Addr,
    /// Pool bonding period
    pub unbonding_period: Option<Duration>,
    /// minimum unbonding cooldown
    pub min_unbonding_cooldown: Option<Duration>,
    /// maximum compound spread
    pub max_swap_spread: Decimal,
}

fn update_config_v0_4_3_to_v0_4_4(
    deps: &mut DepsMut,
) -> Result<(), crate::error::AutocompounderError> {
    let data = deps.storage.get(CONFIG.as_slice()).ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_4_3: OldConfig = from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        staking_contract: config_v0_4_3.staking_contract,
        pool_address: config_v0_4_3.pool_address,
        pool_data: config_v0_4_3.pool_data,
        pool_assets: config_v0_4_3.pool_assets,
        liquidity_token: config_v0_4_3.liquidity_token,
        vault_token: config_v0_4_3.vault_token,
        unbonding_period: config_v0_4_3.unbonding_period,
        min_unbonding_cooldown: config_v0_4_3.min_unbonding_cooldown,
        max_swap_spread: config_v0_4_3.max_swap_spread,
        deposit_enabled: true,
        withdraw_enabled: true,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(())
}

fn update_fee_config_v0_4_3_to_v0_4_4(
    deps: &mut DepsMut,
) -> Result<(), crate::error::AutocompounderError> {
    let data = deps.storage.get(FEE_CONFIG.as_slice()).ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_4_3: OldFeeConfig = from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let fee_config = FeeConfig {
        performance: config_v0_4_3.performance,
        withdrawal: config_v0_4_3.withdrawal,
        deposit: config_v0_4_3.deposit,
        fee_collector_addr: config_v0_4_3.fee_collector_addr,
    };
    FEE_CONFIG.save(deps.storage, &fee_config)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::state::CONFIG;
    use cosmwasm_std::to_vec;
    use cosmwasm_std::{
        testing::mock_dependencies,
        Addr, Decimal,
    };
    use cw_asset::AssetInfoBase;
    use speculoos::{assert_that, prelude::BooleanAssertions};

    type AResult = anyhow::Result<()>;

    fn set_old_config(deps: DepsMut) {
        let config = OldConfig {
            staking_contract: Addr::unchecked("staking_contract"),
            pool_address: Addr::unchecked("pool_address").into(),
            pool_data: abstract_core::objects::PoolMetadata::constant_product(
                "test_dex",
                vec!["asset1", "asset2"],
            ),
            pool_assets: vec![
                AssetInfoBase::Cw20(Addr::unchecked("asset1")),
                AssetInfoBase::Native("asset2".to_string()),
            ],
            liquidity_token: Addr::unchecked("liquidity_token").into(),
            vault_token: Addr::unchecked("vault_token").into(),
            unbonding_period: None,
            min_unbonding_cooldown: None,
            max_swap_spread: Decimal::percent(1),
        };
        deps.storage.set(CONFIG.as_slice(), &to_vec(&config).unwrap());
    }

    fn set_old_fee_config(deps: DepsMut) {
        let config = OldFeeConfig {
            performance: Decimal::percent(1),
            withdrawal: Decimal::percent(1),
            deposit: Decimal::percent(1),
            fee_asset: "fee_asset".into(),
            fee_collector_addr: Addr::unchecked("fee_collector_addr"),
        };
        deps.storage.set(FEE_CONFIG.as_slice(), &to_vec(&config).unwrap());
    }

    #[test]
    fn config_update() -> AResult {
        let mut deps = mock_dependencies();
        set_old_config(deps.as_mut());

        let _resp = update_config_v0_4_3_to_v0_4_4(&mut deps.as_mut()).unwrap();
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.deposit_enabled).is_true();
        assert_that!(config.withdraw_enabled).is_true();
        Ok(())
    }

    #[test]
    fn fee_config_update() -> AResult {
        let mut deps = mock_dependencies();
        set_old_fee_config(deps.as_mut());

        let _resp = update_fee_config_v0_4_3_to_v0_4_4(&mut deps.as_mut()).unwrap();
        let config = FEE_CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.fee_collector_addr).is_equal_to(Addr::unchecked("fee_collector_addr"));
        Ok(())
    }
}
