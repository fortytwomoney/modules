use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::handlers::helpers::check_fee;
use crate::msg::{AutocompounderInstantiateMsg, BondingPeriodSelector, FeeConfig, AUTOCOMPOUNDER};
use crate::state::{Config, CONFIG, DEFAULT_MAX_SPREAD, FEE_CONFIG, VAULT_TOKEN_SYMBOL};
use abstract_core::objects::AnsEntryConvertor;
use abstract_cw_staking::{
    msg::{StakingInfoResponse, StakingQueryMsg},
    CW_STAKING,
};
use abstract_sdk::AdapterInterface;

use abstract_sdk::{
    core::objects::{AssetEntry, DexAssetPairing, LpToken, PoolReference},
    features::AbstractNameService,
};
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use cw_asset::AssetInfo;
use cw_utils::Duration;

use super::helpers::{create_vault_token_submsg, format_native_denom_to_asset};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderInstantiateMsg,
) -> AutocompounderResult {
    // load abstract name service
    let ans = app.name_service(deps.as_ref());

    let AutocompounderInstantiateMsg {
        performance_fees,
        deposit_fees,
        withdrawal_fees,
        commission_addr,
        code_id,
        dex,
        mut pool_assets,
        preferred_bonding_period,
        max_swap_spread,
    } = msg;

    check_fee(performance_fees)?;
    check_fee(deposit_fees)?;
    check_fee(withdrawal_fees)?;

    if pool_assets.len() > 2 {
        return Err(AutocompounderError::PoolWithMoreThanTwoAssets {});
    }

    pool_assets.sort();

    // verify that pool assets are valid
    let _resolved_assets = ans.query(&pool_assets)?;

    let lp_token = LpToken {
        dex: dex.clone(),
        assets: pool_assets.clone(),
    };
    let lp_token_info = ans.query(&lp_token)?;

    let pool_assets_slice = &mut [&pool_assets[0].clone(), &pool_assets[1].clone()];

    // get staking info
    let staking_info = query_staking_info(
        deps.as_ref(),
        &app,
        AnsEntryConvertor::new(lp_token).asset_entry(),
        dex.clone(),
    )?;
    let (unbonding_period, min_unbonding_cooldown) =
        get_unbonding_period_and_min_unbonding_cooldown(
            staking_info.clone(),
            preferred_bonding_period,
        )?;

    let pairing = DexAssetPairing::new(
        pool_assets_slice[0].clone(),
        pool_assets_slice[1].clone(),
        &dex,
    );
    let mut pool_references = ans.query(&pairing)?;
    let pool_reference: PoolReference = pool_references.swap_remove(0);
    // get the pool data
    let mut pool_data = ans.query(&pool_reference.unique_id)?;

    pool_data.assets.sort();

    let resolved_pool_assets = ans.query(&pool_data.assets)?;

    // default max swap spread
    let max_swap_spread =
        max_swap_spread.unwrap_or_else(|| Decimal::percent(DEFAULT_MAX_SPREAD.into()));

    // vault_token will be overwritten in the instantiate reply if we are using a cw20
    let vault_token = if code_id.is_some() {
        AssetInfo::cw20(Addr::unchecked(""))
    } else {
        format_native_denom_to_asset(env.contract.address.as_str(), VAULT_TOKEN_SYMBOL)
    };

    let config: Config = Config {
        vault_token,
        staking_target: staking_info.staking_target,
        liquidity_token: lp_token_info,
        pool_data,
        pool_assets: resolved_pool_assets,
        pool_address: pool_reference.pool_address,
        unbonding_period,
        min_unbonding_cooldown,
        max_swap_spread,
    };

    CONFIG.save(deps.storage, &config)?;

    let fee_config = FeeConfig {
        performance: performance_fees,
        deposit: deposit_fees,
        withdrawal: withdrawal_fees,
        fee_collector_addr: deps.api.addr_validate(&commission_addr)?,
    };

    FEE_CONFIG.save(deps.storage, &fee_config)?;

    // create LP token SubMsg
    let sub_msg = create_vault_token_submsg(
        env.contract.address.to_string(),
        format!("4T2{pairing}"),
        // pool data is too long
        // format!("4T2 Vault Token for {pool_data}"),
        VAULT_TOKEN_SYMBOL.to_string(), // TODO: find a better way to define name and symbol
        code_id, // if code_id is none, submsg will be like normal msg: no reply (for now).
    )?;

    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}

pub fn query_staking_info(
    deps: Deps,
    app: &AutocompounderApp,
    lp_token_name: AssetEntry,
    dex: String,
) -> StdResult<StakingInfoResponse> {
    let adapters = app.adapters(deps);

    let query = StakingQueryMsg::Info {
        provider: dex.clone(),
        staking_token: lp_token_name.clone(),
    };

    let res: StakingInfoResponse = adapters.query(CW_STAKING, query.clone()).map_err(|e| {
        StdError::generic_err(format!(
            "Error querying staking info for {lp_token_name} on {dex}: {e}...{query:?}"
        ))
    })?;
    Ok(res)
}

/// Retrieves the unbonding period and minimum unbonding cooldown based on the staking info and preferred bonding period.
pub fn get_unbonding_period_and_min_unbonding_cooldown(
    staking_info: StakingInfoResponse,
    preferred_bonding_period: BondingPeriodSelector,
) -> Result<(Option<Duration>, Option<Duration>), AutocompounderError> {
    if let (max_claims, Some(mut unbonding_periods)) =
        (staking_info.max_claims, staking_info.unbonding_periods)
    {
        if !all_durations_are_height(&unbonding_periods)
            && !all_durations_are_time(&unbonding_periods)
        {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Unbonding periods are not all heights or all times",
            )));
        }

        sort_unbonding_periods(&mut unbonding_periods);

        let unbonding_duration = match preferred_bonding_period {
            BondingPeriodSelector::Shortest => *unbonding_periods.first().unwrap(),
            BondingPeriodSelector::Longest => *unbonding_periods.last().unwrap(),
            BondingPeriodSelector::Custom(duration) => {
                // check if the duration is in the unbonding periods
                if unbonding_periods.contains(&duration) {
                    duration
                } else {
                    return Err(AutocompounderError::Std(StdError::generic_err(
                        "Custom bonding period is not in the dex's unbonding periods",
                    )));
                }
            }
        };

        let min_unbonding_cooldown =
            compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
        Ok((Some(unbonding_duration), min_unbonding_cooldown))
    } else {
        Ok((None, None))
    }
}

/// computes the minimum cooldown period based on the max claims and unbonding duration.
fn compute_min_unbonding_cooldown(
    max_claims: Option<u32>,
    unbonding_duration: Duration,
) -> Result<Option<Duration>, AutocompounderError> {
    if max_claims.is_none() {
        return Ok(None);
    } else if max_claims == Some(0) {
        return Err(AutocompounderError::Std(StdError::generic_err(
            "Max claims cannot be 0.",
        )));
    }

    let min_unbonding_cooldown = max_claims.map(|max| match &unbonding_duration {
        Duration::Height(block) => Duration::Height(block.saturating_div(max.into())),
        Duration::Time(secs) => Duration::Time(secs.saturating_div(max.into())),
    });
    Ok(min_unbonding_cooldown)
}

/// Sorts the unbonding periods based on their type.
fn sort_unbonding_periods(unbonding_periods: &mut [Duration]) {
    unbonding_periods.sort_by(|a, b| match (a, b) {
        (Duration::Height(a_height), Duration::Height(b_height)) => a_height.cmp(b_height),
        (Duration::Time(a_time), Duration::Time(b_time)) => a_time.cmp(b_time),
        _ => panic!("Mismatched duration types, which should have been checked earlier."),
    });
}

/// Checks if all durations are of type Height.
fn all_durations_are_height(unbonding_periods: &[Duration]) -> bool {
    unbonding_periods
        .iter()
        .all(|x| matches!(x, Duration::Height(_)))
}

/// Checks if all durations are of type Time.
fn all_durations_are_time(unbonding_periods: &[Duration]) -> bool {
    unbonding_periods
        .iter()
        .all(|x| matches!(x, Duration::Time(_)))
}

#[cfg(test)]
mod test {
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_base_mock_querier};
    use abstract_sdk::base::InstantiateEndpoint;
    use abstract_sdk::core as abstract_core;
    use abstract_testing::prelude::{TEST_ANS_HOST, TEST_MODULE_FACTORY};
    const ASTROPORT: &str = "astroport";
    const COMMISSION_RECEIVER: &str = "commission_receiver";
    use crate::test_common::app_init;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        Addr, Decimal,
    };
    use cw20::MinterResponse;
    use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
    use cw_asset::AssetInfo;
    use speculoos::{assert_that, result::ResultAssertions};

    use super::*;

    #[test]
    fn test_app_instantiation() -> anyhow::Result<()> {
        let deps = app_init(false, true);
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        let fee_config = FEE_CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.pool_assets.len()).is_equal_to(2);
        assert_that!(&config.pool_assets).matches(|x| {
            x.contains(&AssetInfo::Native("usd".into()))
                && x.contains(&AssetInfo::Native("eur".into()))
        });
        assert_that!(fee_config).is_equal_to(FeeConfig {
            performance: Decimal::percent(3),
            deposit: Decimal::percent(3),
            withdrawal: Decimal::percent(3),
            fee_collector_addr: Addr::unchecked("commission_receiver".to_string()),
        });
        Ok(())
    }

    #[test]
    fn pool_assets_length_cannot_be_greater_than_2() -> anyhow::Result<()> {
        let mut deps = mock_dependencies();
        let info = mock_info(TEST_MODULE_FACTORY, &[]);

        deps.querier = app_base_mock_querier().build();

        let resp = AUTOCOMPOUNDER_APP.instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            abstract_core::app::InstantiateMsg {
                module: crate::msg::AutocompounderInstantiateMsg {
                    code_id: Some(1),
                    commission_addr: COMMISSION_RECEIVER.to_string(),
                    deposit_fees: Decimal::percent(3),
                    dex: ASTROPORT.to_string(),
                    performance_fees: Decimal::percent(3),
                    pool_assets: vec!["eur".into(), "usd".into(), "juno".into()],
                    withdrawal_fees: Decimal::percent(3),
                    preferred_bonding_period: BondingPeriodSelector::Shortest,
                    max_swap_spread: None,
                },
                base: abstract_core::app::BaseInstantiateMsg {
                    ans_host_address: TEST_ANS_HOST.to_string(),
                },
            },
        );

        assert_that!(resp)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::PoolWithMoreThanTwoAssets {}));
        Ok(())
    }

    #[test]
    fn test_cw_20_init() {
        let pairing = DexAssetPairing::new(
            AssetEntry::from("terra2>astro"),
            AssetEntry::from("terra2>luna"),
            "astroport",
        );
        let name = format!("4T2 {pairing}");

        let msg = TokenInstantiateMsg {
            name,
            symbol: "FORTYTWO".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: "".to_string(),
                cap: None,
            }),
            marketing: None,
        };

        msg.validate().unwrap();
    }

    mod unbonding_period_tests {
        use super::*;

        #[test]
        fn test_all_durations_are_height() {
            let durations = vec![
                Duration::Height(10),
                Duration::Height(20),
                Duration::Height(30),
            ];
            assert_eq!(all_durations_are_height(&durations), true);

            let mixed_durations = vec![
                Duration::Height(10),
                Duration::Time(20),
                Duration::Height(30),
            ];
            assert_eq!(all_durations_are_height(&mixed_durations), false);
        }

        #[test]
        fn test_all_durations_are_time() {
            let durations = vec![Duration::Time(10), Duration::Time(20), Duration::Time(30)];
            assert_eq!(all_durations_are_time(&durations), true);

            let mixed_durations = vec![
                Duration::Height(10),
                Duration::Time(20),
                Duration::Height(30),
            ];
            assert_eq!(all_durations_are_time(&mixed_durations), false);
        }

        #[test]
        fn test_sort_unbonding_periods_height() {
            let mut durations = vec![
                Duration::Height(30),
                Duration::Height(10),
                Duration::Height(20),
            ];
            sort_unbonding_periods(&mut durations);
            assert_eq!(
                durations,
                vec![
                    Duration::Height(10),
                    Duration::Height(20),
                    Duration::Height(30),
                ]
            );
        }

        #[test]
        fn test_sort_unbonding_periods_time() {
            let mut durations = vec![Duration::Time(30), Duration::Time(10), Duration::Time(20)];
            sort_unbonding_periods(&mut durations);
            assert_eq!(
                durations,
                vec![Duration::Time(10), Duration::Time(20), Duration::Time(30),]
            );
        }

        #[test]
        #[should_panic(
            expected = "Mismatched duration types, which should have been checked earlier."
        )]
        fn test_sort_unbonding_periods_mixed() {
            let mut mixed_durations = vec![
                Duration::Height(10),
                Duration::Time(20),
                Duration::Height(30),
            ];
            sort_unbonding_periods(&mut mixed_durations);
        }
    }

    mod cooldown_tests {
        type AResult = anyhow::Result<()>;

        use super::*;
        #[test]
        fn test_compute_min_unbonding_cooldown_height() -> AResult {
            let max_claims = Some(2);
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, Some(Duration::Height(5)));
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_time() -> AResult {
            let max_claims = Some(2);
            let unbonding_duration = Duration::Time(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, Some(Duration::Time(5)));
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_no_max_claims() -> AResult {
            let max_claims = None;
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, None);
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_zero_max_claims() -> AResult {
            let max_claims = Some(0);
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration);
            assert_that!(result)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::Std(_)));
            Ok(())
        }
    }
}
