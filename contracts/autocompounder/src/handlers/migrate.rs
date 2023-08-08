use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{Config, CONFIG};
use abstract_core::objects::{PoolAddress, PoolMetadata};
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
    msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    match msg.version.as_str() {
        "0.5.0" => migrate_from_v0_5_0(&mut deps),
        "0.6.0" => migrate_from_v0_6_0(&mut deps),
        _ => Err(crate::error::AutocompounderError::Std(StdError::generic_err("version migration not supported"))),
    }
}

#[cosmwasm_schema::cw_serde]
pub struct V0_5_0Config {
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

#[cosmwasm_schema::cw_serde]
pub struct V0_6_0Config {
    /// Address of the staking contract
    pub staking_target: StakingTarget,
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
/// which is reflected in the contract update from v0.5.0 to v0.6.0
fn migrate_from_v0_5_0(
    deps: &mut DepsMut,
) -> AutocompounderResult {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_5_0: V0_5_0Config =
        from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        // This is the change from v0.5.0 to v0.6.0
        staking_target: StakingTarget::Contract(config_v0_5_0.staking_contract),
        pool_address: config_v0_5_0.pool_address,
        pool_data: config_v0_5_0.pool_data,
        pool_assets: config_v0_5_0.pool_assets,

        // This is the change from v0.6.0 to v0.7.0
        liquidity_token: cw_asset::AssetInfoBase::Cw20(config_v0_5_0.liquidity_token),
        vault_token: config_v0_5_0.vault_token,
        unbonding_period: config_v0_5_0.unbonding_period,
        min_unbonding_cooldown: config_v0_5_0.min_unbonding_cooldown,
        max_swap_spread: config_v0_5_0.max_swap_spread,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default().add_attribute("migration", "v0.5.0 -> v0.7.0"))
}

fn migrate_from_v0_6_0(
    deps: &mut DepsMut,
) -> AutocompounderResult {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_6_0: V0_6_0Config =
        from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        staking_target: config_v0_6_0.staking_target,
        pool_address: config_v0_6_0.pool_address,
        pool_data: config_v0_6_0.pool_data,
        pool_assets: config_v0_6_0.pool_assets,
        
        // This is the change from v0.6.0 to v0.7.0
        liquidity_token: cw_asset::AssetInfoBase::Cw20(config_v0_6_0.liquidity_token),
        vault_token: config_v0_6_0.vault_token,
        unbonding_period: config_v0_6_0.unbonding_period,
        min_unbonding_cooldown: config_v0_6_0.min_unbonding_cooldown,
        max_swap_spread: config_v0_6_0.max_swap_spread,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default().add_attribute("migration", "v0.6.0 -> v0.7.0"))
}


#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::to_vec;
    use cosmwasm_std::{testing::mock_dependencies, Addr, Decimal};
    use speculoos::assert_that;

    type AResult = anyhow::Result<()>;

    fn set_old_config(deps: DepsMut) {
        let config = V0_5_0Config {
            staking_contract: Addr::unchecked("staking_contract"),
            pool_address: PoolAddress::Id(1),
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
        deps.storage
            .set(CONFIG.as_slice(), &to_vec(&config).unwrap());
    }

    #[test]
    fn config_update() -> AResult {
        let mut deps = mock_dependencies();
        set_old_config(deps.as_mut());

        let _resp = migrate_from_v0_5_0(&mut deps.as_mut()).unwrap();
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.staking_target)
            .is_equal_to(StakingTarget::Contract(Addr::unchecked("staking_contract")));
        Ok(())
    }
}
