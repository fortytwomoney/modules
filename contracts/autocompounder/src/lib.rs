pub mod contract;
mod dependencies;
pub mod error;
mod handlers;
pub mod response;
pub mod state;

// TODO; FIX
// #[cfg(test)]
// #[cfg(not(target_arch = "wasm32"))]
// mod tests;

#[cfg(test)]
mod test_common {
    use abstract_cw_staking_api::msg::{CwStakingQueryMsg, StakingInfoResponse};
    use abstract_sdk::base::InstantiateEndpoint;
    pub use abstract_sdk::core as abstract_core;
    use abstract_sdk::core::{
        module_factory::ContextResponse,
        objects::{PoolMetadata, PoolReference},
        version_control::AccountBase,
    };
    use abstract_testing::{
        addresses::{TEST_MANAGER, TEST_MODULE_FACTORY, TEST_PROXY},
        prelude::{AbstractMockQuerierBuilder, TEST_ANS_HOST},
        MockDeps, MockQuerierBuilder,
    };
    pub use cosmwasm_std::testing::*;
    use cosmwasm_std::{from_binary, to_binary, Addr, Decimal};
    use cw_asset::AssetInfo;
    use cw_utils::Duration;
    use forty_two::autocompounder::BondingPeriodSelector;
    pub use speculoos::prelude::*;

    use crate::contract::AUTO_COMPOUNDER_APP;
    const WYNDEX: &str = "wyndex";
    const COMMISSION_RECEIVER: &str = "commission_receiver";
    const TEST_CW_STAKING_MODULE: &str = "cw_staking";
    const TEST_POOL_ADDR: &str = "test_pool";

    // Mock Querier with a smart-query handler for the module factory
    // Because that query is performed when the App is instantiated to get the manager's address and set it as the Admin
    pub fn app_base_mock_querier() -> MockQuerierBuilder {
        let abstract_env =
            AbstractMockQuerierBuilder::default().account(TEST_MANAGER, TEST_PROXY, 0);
        abstract_env
            .builder()
            .with_smart_handler(TEST_MODULE_FACTORY, |msg| match from_binary(msg).unwrap() {
                abstract_core::module_factory::QueryMsg::Context {} => {
                    let resp = ContextResponse {
                        account_base: Some(AccountBase {
                            manager: Addr::unchecked(TEST_MANAGER),
                            proxy: Addr::unchecked(TEST_PROXY),
                        }),
                        module: None,
                    };
                    Ok(to_binary(&resp).unwrap())
                }
                _ => panic!("unexpected message"),
            })
            .with_smart_handler(TEST_CW_STAKING_MODULE, |msg| {
                match from_binary(msg).unwrap() {
                    abstract_cw_staking_api::msg::QueryMsg::Module(CwStakingQueryMsg::Info {
                        provider: _,
                        staking_token: _,
                    }) => {
                        let resp = StakingInfoResponse {
                            staking_contract_address: Addr::unchecked("staking_addr"),
                            staking_token: AssetInfo::cw20(Addr::unchecked("usd_eur_lp")),
                            unbonding_periods: None,
                            max_claims: None,
                        };
                        Ok(to_binary(&resp).unwrap())
                    }
                    _ => panic!("unexpected message"),
                }
            })
            .with_raw_handler(TEST_ANS_HOST, |key| match key {
                "\0\u{6}assetseur" => Ok(to_binary(&AssetInfo::Native("eur".into())).unwrap()),
                "\0\u{6}assetsusd" => Ok(to_binary(&AssetInfo::Native("usd".into())).unwrap()),
                "\0\u{6}assetswyndex/eur,usd" => {
                    Ok(to_binary(&AssetInfo::cw20(Addr::unchecked("usd_eur_lp"))).unwrap())
                }
                "\0\tcontracts\0\twyndexstaking/wyndex/eur,usd" => {
                    Ok(to_binary(&Addr::unchecked("staking_addr")).unwrap())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{3}usdwyndex" => {
                    Ok(to_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{5}pools\0\0\0\0\0\0\0\0" => Ok(to_binary(&PoolMetadata::new(
                    WYNDEX,
                    abstract_core::objects::PoolType::ConstantProduct,
                    vec!["usd", "eur"],
                ))
                .unwrap()),
                _ => {
                    println!();
                    panic!("Key: {:?} not matched in TEST_ANS mock querier", key);
                }
            })
            // .with_raw_handler(TEST_PROXY, |key| match key {
            //     "admin" => Ok(to_binary(&Some(Addr::unchecked(TEST_MANAGER))).unwrap()),
            //     _ => panic!("unexpected raw key"),
            // })
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::OS_MODULES,
                (
                    "abstract:cw-staking",
                    Addr::unchecked(TEST_CW_STAKING_MODULE),
                )
                    .into(),
            )
    }

    // same as app_base_mock_querier but there is unbonding period for tokens
    pub fn app_base_mock_querier_with_unbonding_period() -> MockQuerierBuilder {
        let abstract_env =
            AbstractMockQuerierBuilder::default().account(TEST_MANAGER, TEST_PROXY, 0);
        abstract_env
            .builder()
            .with_smart_handler(TEST_MODULE_FACTORY, |msg| match from_binary(msg).unwrap() {
                abstract_core::module_factory::QueryMsg::Context {} => {
                    let resp = ContextResponse {
                        account_base: Some(AccountBase {
                            manager: Addr::unchecked(TEST_MANAGER),
                            proxy: Addr::unchecked(TEST_PROXY),
                        }),
                        module: None,
                    };
                    Ok(to_binary(&resp).unwrap())
                }
                _ => panic!("unexpected message"),
            })
            .with_smart_handler(TEST_CW_STAKING_MODULE, |msg| {
                match from_binary(msg).unwrap() {
                    abstract_cw_staking_api::msg::QueryMsg::Module(CwStakingQueryMsg::Info {
                        provider: _,
                        staking_token: _,
                    }) => {
                        let resp = StakingInfoResponse {
                            staking_contract_address: Addr::unchecked("staking_addr"),
                            staking_token: AssetInfo::cw20(Addr::unchecked("usd_eur_lp")),
                            unbonding_periods: Some(vec![Duration::Time(3600)]),
                            max_claims: None,
                        };
                        Ok(to_binary(&resp).unwrap())
                    }
                    _ => panic!("unexpected message"),
                }
            })
            .with_raw_handler(TEST_ANS_HOST, |key| match key {
                "\0\u{6}assetseur" => Ok(to_binary(&AssetInfo::Native("eur".into())).unwrap()),
                "\0\u{6}assetsusd" => Ok(to_binary(&AssetInfo::Native("usd".into())).unwrap()),
                "\0\u{6}assetswyndex/eur,usd" => {
                    Ok(to_binary(&AssetInfo::cw20(Addr::unchecked("usd_eur_lp"))).unwrap())
                }
                "\0\tcontracts\0\twyndexstaking/wyndex/eur,usd" => {
                    Ok(to_binary(&Addr::unchecked("staking_addr")).unwrap())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{3}usdwyndex" => {
                    Ok(to_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{5}pools\0\0\0\0\0\0\0\0" => Ok(to_binary(&PoolMetadata::new(
                    WYNDEX,
                    abstract_core::objects::PoolType::ConstantProduct,
                    vec!["usd", "eur"],
                ))
                .unwrap()),
                _ => {
                    println!();
                    panic!("Key: {:?} not matched in TEST_ANS mock querier", key);
                }
            })
            // .with_raw_handler(TEST_PROXY, |key| match key {
            //     "admin" => Ok(to_binary(&Some(Addr::unchecked(TEST_MANAGER))).unwrap()),
            //     _ => panic!("unexpected raw key"),
            // })
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::OS_MODULES,
                (
                    "abstract:cw-staking",
                    Addr::unchecked(TEST_CW_STAKING_MODULE),
                )
                    .into(),
            )
    }

    pub fn app_init(is_unbonding_period_enabled: bool) -> MockDeps {
        let mut deps = mock_dependencies();
        let info = mock_info(TEST_MODULE_FACTORY, &[]);

        if is_unbonding_period_enabled {
            deps.querier = app_base_mock_querier_with_unbonding_period().build();
        } else {
            deps.querier = app_base_mock_querier().build();
        }

        AUTO_COMPOUNDER_APP
            .instantiate(
                deps.as_mut(),
                mock_env(),
                info,
                abstract_core::app::InstantiateMsg {
                    module: forty_two::autocompounder::AutocompounderInstantiateMsg {
                        code_id: 1,
                        commission_addr: COMMISSION_RECEIVER.to_string(),
                        deposit_fees: Decimal::percent(3),
                        dex: WYNDEX.to_string(),
                        fee_asset: "eur".to_string(),
                        performance_fees: Decimal::percent(3),
                        pool_assets: vec!["eur".into(), "usd".into()],
                        withdrawal_fees: Decimal::percent(3),
                        preferred_bonding_period: BondingPeriodSelector::Shortest,
                    },
                    base: abstract_core::app::BaseInstantiateMsg {
                        ans_host_address: TEST_ANS_HOST.to_string(),
                    },
                },
            )
            .unwrap();

        deps
    }
}
