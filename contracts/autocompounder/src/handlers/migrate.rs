use std::borrow::BorrowMut;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{FeeConfig, FEE_CONFIG, CONFIG, Config};
use abstract_core::objects::{AssetEntry, PoolAddress, PoolMetadata};
use abstract_cw_staking::msg::StakingTarget;
use cosmwasm_std::{from_slice, Addr, Decimal, DepsMut, Env, Response, StdError};
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
    update_config_staking_info_change(&mut deps)?;

    Ok(Response::default())
}

#[cosmwasm_schema::cw_serde]
pub struct V050Config {
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

/// The cw-staking adapter changed from v0.17 to v0.18 and introduced a new type: StakingTarget
fn update_config_staking_info_change(deps: &mut DepsMut) -> Result<(), crate::error::AutocompounderError> {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_5_0: V050Config =
        from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let fee_config = Config {
        staking_target: StakingTarget::Contract(config_v0_5_0.staking_contract),
        pool_address: config_v0_5_0.pool_address,
        pool_data: config_v0_5_0.pool_data,
        pool_assets: config_v0_5_0.pool_assets,
        liquidity_token: config_v0_5_0.liquidity_token,
        vault_token: config_v0_5_0.vault_token,
        unbonding_period: config_v0_5_0.unbonding_period,
        min_unbonding_cooldown: config_v0_5_0.min_unbonding_cooldown,
        max_swap_spread: config_v0_5_0.max_swap_spread,
    };
    CONFIG.save(deps.storage, &fee_config)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::to_vec;
    use cosmwasm_std::{testing::mock_dependencies, Addr, Decimal};
    use speculoos::assert_that;

    type AResult = anyhow::Result<()>;

    fn set_old_config(deps: DepsMut) {
        let config = V050Config {
            staking_contract: Addr::unchecked("staking_contract"),
            pool_address: PoolAddress::Number(1),
            pool_data: PoolMetadata {
                dex: "test".to_string(),
                pool_type: abstract_core::objects::PoolType::ConstantProduct,
                assets: vec![],
            },
            pool_assets: vec![],
            liquidity_token: Addr::unchecked("liquidity_token"),
            vault_token: Addr::unchecked("vault_token"),
            unbonding_period: None,
            min_unbonding_cooldown: None,
            max_swap_spread: Decimal::percent(5),
        };
        deps.storage.set(CONFIG.as_slice(), &to_vec(&config).unwrap());
    }

    #[test]
    fn config_update() -> AResult {
        let mut deps = mock_dependencies();
        set_old_config(deps.as_mut());

        let _resp = update_config_staking_info_change(&mut deps.as_mut()).unwrap();
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.staking_target).is_equal_to(StakingTarget::Contract(Addr::unchecked("staking_contract")));
        Ok(())
    }
}
