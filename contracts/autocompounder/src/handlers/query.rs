use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::state::{
    Claim, Config, FeeConfig, CLAIMS, CONFIG, FEE_CONFIG, LATEST_UNBONDING, PENDING_CLAIMS,
};
use abstract_sdk::features::AccountIdentification;
use abstract_sdk::AdapterInterface;
use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env, Order, StdResult, Uint128};

use crate::msg::AutocompounderQueryMsg;
use abstract_cw_staking::{msg::StakingQueryMsg, CW_STAKING_ADAPTER_ID};
use cw_storage_plus::Bound;
use cw_utils::Expiration;

use super::convert_to_assets;
use super::helpers::{vault_token_balance, vault_token_total_supply};

const DEFAULT_PAGE_SIZE: u8 = 5;
const MAX_PAGE_SIZE: u8 = 20;

/// Handle queries sent to this app.
pub fn query_handler(
    deps: Deps,
    _env: Env,
    app: &AutocompounderApp,
    msg: AutocompounderQueryMsg,
) -> AutocompounderResult<Binary> {
    match msg {
        AutocompounderQueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        AutocompounderQueryMsg::FeeConfig {} => Ok(to_json_binary(&query_fee_config(deps)?)?),
        AutocompounderQueryMsg::PendingClaims { address } => {
            Ok(to_json_binary(&query_pending_claims(deps, address)?)?)
        }
        AutocompounderQueryMsg::AllPendingClaims { start_after, limit } => Ok(to_json_binary(
            &query_all_pending_claims(deps, start_after, limit)?,
        )?),
        AutocompounderQueryMsg::Claims { address } => {
            Ok(to_json_binary(&query_claims(deps, address)?)?)
        }
        AutocompounderQueryMsg::AllClaims { start_after, limit } => Ok(to_json_binary(
            &query_all_claims(deps, start_after, limit)?,
        )?),
        AutocompounderQueryMsg::LatestUnbonding {} => {
            Ok(to_json_binary(&query_latest_unbonding(deps)?)?)
        }
        AutocompounderQueryMsg::TotalLpPosition {} => {
            Ok(to_json_binary(&query_total_lp_position(app, deps)?)?)
        }
        AutocompounderQueryMsg::Balance { address } => {
            Ok(to_json_binary(&query_balance(deps, address)?)?)
        }
        AutocompounderQueryMsg::TotalSupply {} => Ok(to_json_binary(&query_total_supply(deps)?)?),
        AutocompounderQueryMsg::AssetsPerShares { shares } => Ok(to_json_binary(
            &query_assets_per_shares(app, deps, shares)?,
        )?),
    }
}

/// Returns the current configuration.
pub fn query_config(deps: Deps) -> AutocompounderResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    // crate ConfigResponse from config
    Ok(config)
}

pub fn query_fee_config(deps: Deps) -> AutocompounderResult<FeeConfig> {
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    Ok(fee_config)
}

// write query functions for all State const variables: Claims, PendingClaims, LatestUnbonding

pub fn query_pending_claims(deps: Deps, address: Addr) -> AutocompounderResult<Uint128> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(Uint128::zero());
    }

    let pending_claims = PENDING_CLAIMS.may_load(deps.storage, address)?;
    Ok(pending_claims.unwrap_or_default())
}

pub fn query_all_pending_claims(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u8>,
) -> AutocompounderResult<Vec<(Addr, Uint128)>> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(vec![]);
    }

    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.as_bytes().to_vec()));
    let claims = PENDING_CLAIMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, amount)| -> StdResult<(Addr, Uint128)> { Ok((addr, amount)) })?
        })
        .collect::<StdResult<Vec<(Addr, Uint128)>>>()?;

    Ok(claims)
}

pub fn query_claims(deps: Deps, address: Addr) -> AutocompounderResult<Vec<Claim>> {
    let claims = CLAIMS.may_load(deps.storage, address)?.unwrap_or_default();
    Ok(claims)
}

pub fn query_all_claims(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u8>,
) -> AutocompounderResult<Vec<(Addr, Vec<Claim>)>> {
    let bonding_period = CONFIG.load(deps.storage)?.unbonding_period;
    if bonding_period.is_none() {
        return Ok(vec![]);
    }

    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.as_bytes().to_vec()));
    let claims = CLAIMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            item.map(|(addr, claims)| -> StdResult<(Addr, Vec<Claim>)> { Ok((addr, claims)) })
        }?)
        .collect::<StdResult<Vec<(Addr, Vec<Claim>)>>>()?;

    Ok(claims)
}

pub fn query_latest_unbonding(deps: Deps) -> AutocompounderResult<Expiration> {
    let latest_unbonding = LATEST_UNBONDING.load(deps.storage)?;
    Ok(latest_unbonding)
}

pub fn query_total_lp_position(
    app: &AutocompounderApp,
    deps: Deps,
) -> AutocompounderResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let adapters = app.adapters(deps);

    // query staking api for total lp tokens

    let query = StakingQueryMsg::Staked {
        provider: config.pool_data.dex.clone(),
        stakes: vec![config.lp_asset_entry()],
        staker_address: app.proxy_address(deps)?.to_string(),
        unbonding_period: config.unbonding_period,
    };
    let res: abstract_cw_staking::msg::StakeResponse =
        adapters.query(CW_STAKING_ADAPTER_ID, query)?;
    let amount = res.amounts.first().ok_or(AutocompounderError::Std(
        cosmwasm_std::StdError::generic_err("No amount found"),
    ))?;
    Ok(*amount)
}

pub fn query_balance(deps: Deps, address: Addr) -> AutocompounderResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    vault_token_balance(deps, &config, address)
}

pub fn query_total_supply(deps: Deps) -> AutocompounderResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    vault_token_total_supply(deps, &config)
}

pub fn query_assets_per_shares(
    app: &AutocompounderApp,
    deps: Deps,
    shares: Option<Uint128>,
) -> AutocompounderResult<Uint128> {
    let shares = if let Some(shares) = shares {
        shares
    } else {
        Uint128::one()
    };

    let total_lp_position = query_total_lp_position(app, deps)?;
    let total_supply = query_total_supply(deps)?;
    let assets = convert_to_assets(shares, total_lp_position, total_supply);

    Ok(assets)
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::test_common::app_init;
    use abstract_core::objects::pool_id::PoolAddressBase;
    use abstract_core::objects::{AssetEntry, PoolMetadata};
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::Addr;
    use cosmwasm_std::Decimal;

    use cw_utils::{Duration, Expiration};
    use speculoos::assert_that;

    fn default_config() -> Config {
        let assets = vec![AssetEntry::new("juno>juno")];

        Config {
            pool_address: PoolAddressBase::Contract(Addr::unchecked("pool_address")),
            pool_data: PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                assets,
            ),
            pool_assets: vec![],
            liquidity_token: cw_asset::AssetInfoBase::Cw20(Addr::unchecked("liquidity_token")),
            vault_token: cw_asset::AssetInfoBase::Cw20(Addr::unchecked("vault_token")),
            unbonding_period: Some(Duration::Time(100)),
            min_unbonding_cooldown: Some(Duration::Time(10)),
            max_swap_spread: Decimal::percent(50),
        }
    }

    mod claims {
        use cosmwasm_std::Timestamp;

        use super::*;

        #[test]
        fn test_query_claims() {
            let _config = default_config();
            let mut deps = app_init(true, true);
            let claim = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(100)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };
            let claim2 = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(200)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };
            let expected_claims = &vec![claim, claim2];

            let user = Addr::unchecked("user");
            CLAIMS
                .save(deps.as_mut().storage, user.clone(), expected_claims)
                .unwrap();

            let claims = query_claims(deps.as_ref(), user).unwrap();
            assert_eq!(claims.len(), 2);
            assert_that!(claims).is_equal_to(expected_claims)
        }

        #[test]
        fn test_query_all_claims() {
            let mut deps = app_init(true, true);

            // Set up some claims
            let claim1 = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(100)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };
            let claim2 = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(200)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };
            let claim3 = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(300)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };
            let claim4 = Claim {
                unbonding_timestamp: Expiration::AtTime(Timestamp::from_seconds(400)),
                amount_of_vault_tokens_to_burn: 1000u128.into(),
                amount_of_lp_tokens_to_unbond: 1000u128.into(),
            };

            let user1 = Addr::unchecked("user1");
            let user2 = Addr::unchecked("user2");
            let user3 = Addr::unchecked("user3");
            let _user4 = Addr::unchecked("user4");

            let user1_claims = &vec![claim1, claim2];
            let user2_claims = &vec![claim3];
            let user3_claims = &vec![claim4];

            CLAIMS
                .save(deps.as_mut().storage, user1.clone(), &user1_claims.clone())
                .unwrap();
            CLAIMS
                .save(deps.as_mut().storage, user2.clone(), &user2_claims.clone())
                .unwrap();
            CLAIMS
                .save(deps.as_mut().storage, user3.clone(), &user3_claims.clone())
                .unwrap();

            // Test with no pagination
            let claims = query_all_claims(deps.as_ref(), None, None).unwrap();
            assert_eq!(claims.len(), 3);
            assert_eq!(claims[0].0, user1);
            assert_eq!(claims[1].0, user2);
            assert_eq!(claims[2].0, user3);
            assert_that!(claims[0].1).is_equal_to(user1_claims.clone());
            assert_that!(claims[1].1).is_equal_to(user2_claims.clone());
            assert_that!(claims[2].1).is_equal_to(user3_claims.clone());

            // Test with pagination
            let claims = query_all_claims(deps.as_ref(), None, Some(2)).unwrap();
            assert_eq!(claims.len(), 2);
            assert_eq!(claims[0].0, user1);
            assert_eq!(claims[1].0, user2);
            assert_that!(claims[0].1).is_equal_to(&user1_claims.clone());
            assert_that!(claims[1].1).is_equal_to(&user2_claims.clone());

            // Test with pagination and start_after
            let claims = query_all_claims(deps.as_ref(), Some(user1), Some(2)).unwrap();
            assert_eq!(claims.len(), 2);
            assert_eq!(claims[0].0, user2);
            assert_that!(claims[0].1).is_equal_to(user2_claims);
        }
    }

    mod unbonding {
        use super::*;

        #[test]
        fn test_query_latest_unbonding() {
            let mut deps = app_init(true, true);
            let expiration = Expiration::AtHeight(10);

            // Store the latest unbonding expiration in storage
            LATEST_UNBONDING
                .save(deps.as_mut().storage, &expiration)
                .unwrap();

            // Query the latest unbonding expiration
            let result = query_latest_unbonding(deps.as_ref()).unwrap();

            // Check that the result matches the stored expiration
            assert_eq!(result, expiration);
        }
    }

    mod vault_token {
        use cw_asset::AssetInfo;

        use crate::test_common::TEST_VAULT_TOKEN;

        use super::*;

        #[test]
        fn test_query_balance() {
            let mut deps = app_init(false, true);

            let vault_balance = Uint128::new(1000);
            let mut config = default_config();
            config.vault_token = AssetInfo::cw20(Addr::unchecked(TEST_VAULT_TOKEN));
            CONFIG.save(deps.as_mut().storage, &config).unwrap();

            let address = Addr::unchecked("addr0001");

            let balance = query_balance(deps.as_ref(), address).unwrap();
            assert_eq!(balance, vault_balance);
        }

        #[test]
        fn test_query_total_supply() {
            let mut deps = app_init(false, true);

            let vault_balance = Uint128::new(1000);
            let mut config = default_config();
            config.vault_token = AssetInfo::cw20(Addr::unchecked(TEST_VAULT_TOKEN));
            CONFIG.save(deps.as_mut().storage, &config).unwrap();

            let total_supply = query_total_supply(deps.as_ref()).unwrap();
            assert_eq!(total_supply, vault_balance);
        }

        #[test]
        fn test_query_total_lp_position() {
            let mut deps = app_init(false, true);
            let _info = mock_info("test", &[]);

            let app = AutocompounderApp::new("test", "test_version", None);

            let lp_balance = Uint128::new(100);
            let mut config = default_config();
            config.vault_token = AssetInfo::cw20(Addr::unchecked(TEST_VAULT_TOKEN));
            CONFIG.save(deps.as_mut().storage, &config).unwrap();

            let total_lp_position = query_total_lp_position(&app, deps.as_ref()).unwrap();
            assert_eq!(total_lp_position, lp_balance);
        }

        #[test]
        fn test_query_assets_per_shares() {
            let mut deps = app_init(false, true);
            let app = AutocompounderApp::new("test", "test_version", None);

            let assets_per_share =
                convert_to_assets(1000u128.into(), 100u128.into(), 1000u128.into());
            let mut config = default_config();
            config.vault_token = AssetInfo::cw20(Addr::unchecked(TEST_VAULT_TOKEN));
            CONFIG.save(deps.as_mut().storage, &config).unwrap();

            let result =
                query_assets_per_shares(&app, deps.as_ref(), Some(1000u128.into())).unwrap();
            assert_eq!(result, assets_per_share);
        }
    }
}
