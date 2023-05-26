use std::borrow::BorrowMut;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{Config, FeeConfig, CONFIG, FEE_CONFIG};
use autocompounder_v0_4_3::state::{CONFIG as CONFIG_V0_4_3, FEE_CONFIG as FEE_CONFIG_V043};
use cosmwasm_std::{DepsMut, Env, Response};

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

fn update_config_v0_4_3_to_v0_4_4(
    _deps: &mut DepsMut,
) -> Result<(), crate::error::AutocompounderError> {
    let config_v0_4_3 = CONFIG_V0_4_3.load(_deps.storage)?;
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
    CONFIG.save(_deps.storage, &config)?;
    Ok(())
}

fn update_fee_config_v0_4_3_to_v0_4_4(
    deps: &mut DepsMut,
) -> Result<(), crate::error::AutocompounderError> {
    let config = FEE_CONFIG_V043.load(deps.storage)?;
    let config = FeeConfig {
        performance: config.performance,
        withdrawal: config.withdrawal,
        deposit: config.deposit,
        fee_collector_addr: config.fee_collector_addr,
    };
    FEE_CONFIG.save(deps.storage, &config)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::state::CONFIG;
    use autocompounder_v0_4_3::state::Config as Config_v0_4_3;
    use autocompounder_v0_4_3::state::FeeConfig as FeeConfig_v0_4_3;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        Addr, Decimal,
    };
    use cw_asset::AssetInfoBase;
    use speculoos::{assert_that, prelude::BooleanAssertions};

    type AResult = anyhow::Result<()>;

    fn set_old_config(deps: DepsMut) {
        let config = Config_v0_4_3 {
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
        CONFIG_V0_4_3.save(deps.storage, &config).unwrap();
    }

    fn set_old_fee_config(deps: DepsMut) {
        let config = FeeConfig_v0_4_3 {
            performance: Decimal::percent(1),
            withdrawal: Decimal::percent(1),
            deposit: Decimal::percent(1),
            fee_asset: "fee_asset".into(),
            fee_collector_addr: Addr::unchecked("fee_collector_addr"),
        };
        FEE_CONFIG_V043.save(deps.storage, &config).unwrap();
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
