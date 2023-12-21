use crate::contract::{AutocompounderApp, AutocompounderResult, MODULE_VERSION};
use crate::error::AutocompounderError;
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{Claim, Config, CLAIMS, CONFIG, PENDING_CLAIMS};
use abstract_core::objects::{PoolAddress, PoolMetadata};
use abstract_cw_staking::msg::StakingTarget;
use cosmwasm_std::{from_json, Addr, Decimal, DepsMut, Env, Response, StdError, Uint128};
use cw_asset::AssetInfo;
use cw_storage_plus::Map;
use cw_utils::Duration;

pub const CURRENT_VERSION: &str = MODULE_VERSION;
/// Unused for now but provided here as an example
/// Contract version is migrated automatically
pub fn migrate_handler(
    mut deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    match msg.version.as_str() {
        "0.5.0" => {
            migrate_from_v0_5_0(&mut deps)?;
            migrate_from_v0_7_claims(&mut deps)?;
            migrate_from_v0_7_pending_claims(&mut deps)?;
            Ok(Response::default()
                .add_attribute("migration", format!("v0.5.0 -> ${}", CURRENT_VERSION)))
        }
        "0.6.0" => {
            migrate_from_v0_6_0(&mut deps)?;
            migrate_from_v0_7_claims(&mut deps)?;
            migrate_from_v0_7_pending_claims(&mut deps)?;
            Ok(Response::default()
                .add_attribute("migration", format!("v0.6.0 -> ${}", CURRENT_VERSION)))
        }
        "0.7.0" | "0.7.1" => {
            migrate_from_v0_7_config(&mut deps)?;
            migrate_from_v0_7_claims(&mut deps)?;
            migrate_from_v0_7_pending_claims(&mut deps)?;
            Ok(Response::default()
                .add_attribute("migration", format!("v0.7.- -> ${}", CURRENT_VERSION)))
        }
        "0.8.0" => Ok(Response::default()
            .add_attribute("migration", format!("v0.8.0 -> ${}", CURRENT_VERSION))),
        _ => Err(crate::error::AutocompounderError::Std(
            StdError::generic_err("version migration not supported"),
        )),
    }
}

pub const V0_7_PENDING_CLAIMS: Map<String, Uint128> = Map::new("pending_claims");
pub const V0_7_CLAIMS: Map<String, Vec<Claim>> = Map::new("claims");

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
#[cosmwasm_schema::cw_serde]
pub struct V0_7_0Config {
    /// Address of the staking contract
    pub staking_target: StakingTarget,
    /// Pool address (number or Address)
    pub pool_address: PoolAddress,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Resolved pool assets
    pub pool_assets: Vec<AssetInfo>,
    /// Address of the LP token contract
    pub liquidity_token: AssetInfo,
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
fn migrate_from_v0_5_0(deps: &mut DepsMut) -> Result<(), AutocompounderError> {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_5_0: V0_5_0Config =
        from_json(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        // This is the change from v0.5.0 to v0.6.0
        // staking_target: StakingTarget::Contract(config_v0_5_0.staking_contract),
        pool_address: config_v0_5_0.pool_address,
        pool_data: config_v0_5_0.pool_data,
        pool_assets: config_v0_5_0.pool_assets,

        // This is the change from v0.6.0 to v0.7.0
        liquidity_token: cw_asset::AssetInfoBase::Cw20(config_v0_5_0.liquidity_token),
        vault_token: cw_asset::AssetInfoBase::Cw20(config_v0_5_0.vault_token),
        unbonding_period: config_v0_5_0.unbonding_period,
        min_unbonding_cooldown: config_v0_5_0.min_unbonding_cooldown,
        max_swap_spread: config_v0_5_0.max_swap_spread,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(())
}

fn migrate_from_v0_6_0(deps: &mut DepsMut) -> Result<(), AutocompounderError> {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_6_0: V0_6_0Config =
        from_json(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        pool_address: config_v0_6_0.pool_address,
        pool_data: config_v0_6_0.pool_data,
        pool_assets: config_v0_6_0.pool_assets,

        // This is the change from v0.6.0 to v0.7.0
        liquidity_token: cw_asset::AssetInfoBase::Cw20(config_v0_6_0.liquidity_token),
        vault_token: cw_asset::AssetInfoBase::Cw20(config_v0_6_0.vault_token),
        unbonding_period: config_v0_6_0.unbonding_period,
        min_unbonding_cooldown: config_v0_6_0.min_unbonding_cooldown,
        max_swap_spread: config_v0_6_0.max_swap_spread,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(())
}

fn migrate_from_v0_7_pending_claims(deps: &mut DepsMut) -> Result<(), AutocompounderError> {
    // load all currently pending claims
    let pending_claims_v0_7 = V0_7_PENDING_CLAIMS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|c| {
            let (addr_str, amount) = c?;
            let addr = deps.api.addr_validate(&addr_str)?;
            Ok((addr, amount))
        })
        .collect::<Result<Vec<(Addr, Uint128)>, StdError>>()
        .map_err(AutocompounderError::Std)?;

    // clear the old state
    PENDING_CLAIMS.clear(deps.storage);

    // save the new state
    for (addr, amount) in pending_claims_v0_7 {
        PENDING_CLAIMS.save(deps.storage, addr, &amount)?;
    }

    Ok(())
}

fn migrate_from_v0_7_config(deps: &mut DepsMut) -> Result<(), AutocompounderError> {
    let data = deps
        .storage
        .get(CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_7: V0_7_0Config =
        from_json(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let config = Config {
        pool_address: config_v0_7.pool_address,
        pool_data: config_v0_7.pool_data,
        pool_assets: config_v0_7.pool_assets,
        liquidity_token: config_v0_7.liquidity_token,
        vault_token: cw_asset::AssetInfoBase::cw20(config_v0_7.vault_token),
        unbonding_period: config_v0_7.unbonding_period,
        min_unbonding_cooldown: config_v0_7.min_unbonding_cooldown,
        max_swap_spread: config_v0_7.max_swap_spread,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(())
}

fn migrate_from_v0_7_claims(deps: &mut DepsMut) -> Result<(), AutocompounderError> {
    // load all currently pending claims
    let claims_v0_7 = V0_7_CLAIMS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|c| {
            let (addr_str, amount) = c?;
            let addr = deps.api.addr_validate(&addr_str)?;
            Ok((addr, amount))
        })
        .collect::<Result<Vec<(Addr, Vec<Claim>)>, StdError>>()
        .map_err(AutocompounderError::Std)?;

    // clear the old state
    CLAIMS.clear(deps.storage);

    // save the new state
    for (addr, claims) in claims_v0_7 {
        CLAIMS.save(deps.storage, addr, &claims)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::contract::AUTOCOMPOUNDER_APP;
    use crate::test_common::app_init;

    use super::*;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::to_json_vec;
    use cosmwasm_std::{testing::mock_dependencies, Addr, Decimal};
    use cw_utils::Expiration;
    use speculoos::assert_that;
    use speculoos::prelude::OptionAssertions;

    type AResult = anyhow::Result<()>;

    fn set_v0_5_0_config(deps: DepsMut) {
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
            .set(CONFIG.as_slice(), &to_json_vec(&config).unwrap());
    }

    fn set_v0_6_0_config(deps: DepsMut) {
        let config = V0_6_0Config {
            staking_target: StakingTarget::Contract(Addr::unchecked("staking_contract")),
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
            .set(CONFIG.as_slice(), &to_json_vec(&config).unwrap());
    }

    fn set_v0_7_0_config(deps: DepsMut) {
        let config = V0_7_0Config {
            staking_target: StakingTarget::Contract(Addr::unchecked("staking_contract")),
            pool_address: PoolAddress::Id(1),
            pool_data: PoolMetadata {
                dex: "test".to_string(),
                pool_type: abstract_core::objects::PoolType::ConstantProduct,
                assets: vec![],
            },
            pool_assets: vec![],
            liquidity_token: AssetInfo::Cw20(Addr::unchecked("liquidity_token")),
            vault_token: Addr::unchecked("vault_token"),
            unbonding_period: None,
            min_unbonding_cooldown: None,
            max_swap_spread: Decimal::percent(5),
        };
        deps.storage
            .set(CONFIG.as_slice(), &to_json_vec(&config).unwrap());
    }

    #[test]
    fn test_migrate_from_v0_5_0() -> AResult {
        let mut deps = mock_dependencies();
        set_v0_5_0_config(deps.as_mut());

        migrate_from_v0_5_0(&mut deps.as_mut()).unwrap();
        let _config = CONFIG.load(deps.as_ref().storage).unwrap();
        Ok(())
    }

    #[test]
    fn test_migrate_from_v0_6_0() -> AResult {
        let mut deps = mock_dependencies();
        set_v0_6_0_config(deps.as_mut());

        migrate_from_v0_6_0(&mut deps.as_mut()).unwrap();
        let _config = CONFIG.load(deps.as_ref().storage).unwrap();
        Ok(())
    }

    #[test]
    fn migrate_from_v0_7_claims_test() -> AResult {
        let mut deps = mock_dependencies();
        let addr = Addr::unchecked("addr");
        let addr2 = Addr::unchecked("addr2");
        let addr3 = Addr::unchecked("addr3");

        let claim1 = Claim {
            unbonding_timestamp: Expiration::AtHeight(1),
            amount_of_vault_tokens_to_burn: 100u128.into(),
            amount_of_lp_tokens_to_unbond: 10u128.into(),
        };

        let claim2 = Claim {
            unbonding_timestamp: Expiration::AtHeight(2),
            amount_of_vault_tokens_to_burn: 200u128.into(),
            amount_of_lp_tokens_to_unbond: 20u128.into(),
        };

        V0_7_CLAIMS.save(
            deps.as_mut().storage,
            addr.to_string(),
            &vec![claim1.clone()],
        )?;
        V0_7_CLAIMS.save(
            deps.as_mut().storage,
            addr2.to_string(),
            &vec![claim1.clone(), claim2.clone()],
        )?;

        migrate_from_v0_7_claims(&mut deps.as_mut()).unwrap();
        let claims = CLAIMS.load(deps.as_ref().storage, addr).unwrap();
        assert_that!(claims).is_equal_to(vec![claim1.clone()]);

        let claims = CLAIMS.load(deps.as_ref().storage, addr2).unwrap();
        assert_that!(claims).is_equal_to(vec![claim1, claim2]);
        let res = CLAIMS.may_load(deps.as_ref().storage, addr3)?;
        assert_that!(res).is_none();

        Ok(())
    }

    #[test]
    fn migrate_from_v0_7_pending_claims_test() -> AResult {
        let mut deps = mock_dependencies();
        let addr = Addr::unchecked("addr");
        let addr2 = Addr::unchecked("addr2");
        let addr3 = Addr::unchecked("addr3");

        let amount1 = Uint128::from(100u128);
        let amount2 = Uint128::from(200u128);

        V0_7_PENDING_CLAIMS.save(deps.as_mut().storage, addr.to_string(), &amount1)?;
        V0_7_PENDING_CLAIMS.save(deps.as_mut().storage, addr2.to_string(), &amount2)?;

        migrate_from_v0_7_pending_claims(&mut deps.as_mut()).unwrap();
        let claims = PENDING_CLAIMS.load(deps.as_ref().storage, addr).unwrap();
        assert_that!(claims).is_equal_to(amount1);

        let claims = PENDING_CLAIMS.load(deps.as_ref().storage, addr2).unwrap();
        assert_that!(claims).is_equal_to(amount2);

        let res = PENDING_CLAIMS.may_load(deps.as_ref().storage, addr3)?;
        assert_that!(res).is_none();

        Ok(())
    }

    #[test]
    fn full_migration_from_v7() -> AResult {
        let mut deps = app_init(false, true);
        set_v0_7_0_config(deps.as_mut());

        let addr = Addr::unchecked("addr");
        let addr2 = Addr::unchecked("addr2");
        let addr3 = Addr::unchecked("addr3");

        let amount1 = Uint128::from(100u128);
        let amount2 = Uint128::from(200u128);

        V0_7_PENDING_CLAIMS.save(deps.as_mut().storage, addr.to_string(), &amount1)?;
        V0_7_PENDING_CLAIMS.save(deps.as_mut().storage, addr2.to_string(), &amount2)?;

        let claim1 = Claim {
            unbonding_timestamp: Expiration::AtHeight(1),
            amount_of_vault_tokens_to_burn: 100u128.into(),
            amount_of_lp_tokens_to_unbond: 10u128.into(),
        };

        let claim2 = Claim {
            unbonding_timestamp: Expiration::AtHeight(2),
            amount_of_vault_tokens_to_burn: 200u128.into(),
            amount_of_lp_tokens_to_unbond: 20u128.into(),
        };

        V0_7_CLAIMS.save(
            deps.as_mut().storage,
            addr.to_string(),
            &vec![claim1.clone()],
        )?;
        V0_7_CLAIMS.save(
            deps.as_mut().storage,
            addr2.to_string(),
            &vec![claim1.clone(), claim2.clone()],
        )?;

        let migrate_msg = AutocompounderMigrateMsg {
            version: "0.7.0".to_string(),
        };

        migrate_handler(deps.as_mut(), mock_env(), AUTOCOMPOUNDER_APP, migrate_msg)?;

        let claims = PENDING_CLAIMS
            .load(deps.as_ref().storage, addr.clone())
            .unwrap();
        assert_that!(claims).is_equal_to(amount1);
        let claims = PENDING_CLAIMS
            .load(deps.as_ref().storage, addr2.clone())
            .unwrap();
        assert_that!(claims).is_equal_to(amount2);
        let res = PENDING_CLAIMS.may_load(deps.as_ref().storage, addr3.clone())?;
        assert_that!(res).is_none();

        let claims = CLAIMS.load(deps.as_ref().storage, addr).unwrap();
        assert_that!(claims).is_equal_to(vec![claim1.clone()]);
        let claims = CLAIMS.load(deps.as_ref().storage, addr2).unwrap();
        assert_that!(claims).is_equal_to(vec![claim1, claim2]);
        let res = CLAIMS.may_load(deps.as_ref().storage, addr3)?;
        assert_that!(res).is_none();

        Ok(())
    }
}
