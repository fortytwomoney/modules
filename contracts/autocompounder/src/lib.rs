pub mod contract;
mod dependencies;
pub mod error;
mod handlers;
pub mod msg;
pub mod response;
pub mod state;

pub use crate::handlers::swap_rewards;

#[cfg(feature = "interface")]
pub mod interface;
pub mod kujira_tx;

#[cfg(test)]
mod test_common {
    use crate::msg::BondingData;

    use abstract_cw_staking::msg::{
        StakeResponse, StakingInfo, StakingInfoResponse, StakingQueryMsg, StakingTarget,
    };
    use abstract_dex_adapter::msg::DexQueryMsg;
    use abstract_sdk::base::InstantiateEndpoint;
    pub use abstract_sdk::core as abstract_core;
    use abstract_sdk::core::{
        objects::{PoolMetadata, PoolReference},
        version_control::AccountBase,
    };
    use abstract_testing::{
        addresses::{TEST_MANAGER, TEST_MODULE_FACTORY, TEST_PROXY},
        prelude::{
            AbstractMockQuerierBuilder, TEST_ACCOUNT_ID, TEST_ANS_HOST, TEST_DEX,
            TEST_VERSION_CONTROL,
        },
        MockDeps, MockQuerierBuilder,
    };
    pub use cosmwasm_std::testing::*;
    use cosmwasm_std::{from_json, to_json_binary, Addr, Decimal, StdError, Uint128};
    use cw_asset::AssetInfo;
    use cw_utils::Duration;

    use crate::contract::AUTOCOMPOUNDER_APP;
    const WYNDEX: &str = "wyndex";
    const COMMISSION_RECEIVER: &str = "commission_receiver";
    const TEST_CW_STAKING_MODULE: &str = "cw_staking";
    const TEST_POOL_ADDR: &str = "test_pool";
    pub const TEST_VAULT_TOKEN: &str = "test_vault_token";
    pub const SHORT_UNBONDING_PERIOD: Duration = Duration::Time(3600);
    pub const LONG_UNBONDING_PERIOD: Duration = Duration::Time(7200);
    pub const MAX_CLAIMS_PER_ADDRESS: u32 = 7;

    // Mock Querier with a smart-query handler for the module factory
    // Because that query is performed when the App is instantiated to get the manager's address and set it as the Admin
    pub fn app_base_mock_querier() -> MockQuerierBuilder {
        let abstract_env = AbstractMockQuerierBuilder::default().account(
            TEST_MANAGER,
            TEST_PROXY,
            TEST_ACCOUNT_ID,
        );
        abstract_env
            .builder()
            .with_smart_handler(TEST_CW_STAKING_MODULE, |msg| {
                match from_json(msg).unwrap() {
                    abstract_cw_staking::msg::QueryMsg::Module(StakingQueryMsg::Info {
                        provider: _,
                        staking_tokens: _,
                    }) => {
                        let resp = StakingInfoResponse {
                            infos: vec![StakingInfo {
                                staking_target: StakingTarget::Contract(Addr::unchecked(
                                    "staking_addr",
                                )),
                                staking_token: AssetInfo::cw20(Addr::unchecked("usd_eur_lp")),
                                unbonding_periods: None,
                                max_claims: None,
                            }],
                        };
                        Ok(to_json_binary(&resp).unwrap())
                    }
                    abstract_cw_staking::msg::QueryMsg::Module(StakingQueryMsg::Staked {
                        provider: _,
                        stakes: _,
                        staker_address: _,
                        unbonding_period: _,
                    }) => {
                        let resp = StakeResponse {
                            amounts: vec![Uint128::new(100)],
                        };
                        Ok(to_json_binary(&resp).unwrap())
                    }
                    _ => panic!("unexpected message"),
                }
            })
            .with_smart_handler(TEST_DEX, |msg| match from_json(msg).unwrap() {
                abstract_dex_adapter::msg::QueryMsg::Module(DexQueryMsg::SimulateSwap {
                    offer_asset: _,
                    ask_asset: _,
                    dex: _,
                }) => {
                    let resp = "hello darkness my old friend";
                    Ok(to_json_binary(&resp).unwrap())
                }
                _ => panic!("unexpected message"),
            })
            .with_smart_handler(TEST_VAULT_TOKEN, |msg| match from_json(msg).unwrap() {
                cw20::Cw20QueryMsg::Balance { address: _ } => {
                    Ok(to_json_binary(&cw20::BalanceResponse {
                        balance: Uint128::new(1000),
                    })
                    .unwrap())
                }
                cw20::Cw20QueryMsg::TokenInfo {} => Ok(to_json_binary(&cw20::TokenInfoResponse {
                    name: "test_vault_token".to_string(),
                    symbol: "test_vault_token".to_string(),
                    decimals: 6,
                    total_supply: Uint128::new(1000),
                })
                .unwrap()),
                _ => panic!("unexpected message"),
            })
            .with_raw_handler(TEST_ANS_HOST, |key| match key {
                "\0\u{6}assetseur_usd_lp" => {
                    Ok(to_json_binary(&AssetInfo::cw20(Addr::unchecked("eur_usd_lp"))).unwrap())
                }
                "\0\u{6}assetsnoteur_usd_lp" => {
                    Ok(to_json_binary(&AssetInfo::cw20(Addr::unchecked("noteur_usd_lp"))).unwrap())
                }
                "\0\u{6}assetseur" => Ok(to_json_binary(&AssetInfo::Native("eur".into())).unwrap()),
                "\0\u{6}assetsusd" => Ok(to_json_binary(&AssetInfo::Native("usd".into())).unwrap()),
                "\0\u{6}assetswyndex/eur,usd" => {
                    Ok(to_json_binary(&AssetInfo::cw20(Addr::unchecked("usd_eur_lp"))).unwrap())
                }
                "\0\tcontracts\0\twyndexstaking/wyndex/eur,usd" => {
                    Ok(to_json_binary(&Addr::unchecked("staking_addr")).unwrap())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{4}wyndwyndex" => {
                    Err(StdError::generic_err("").to_string())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{3}xrpwyndex" => {
                    Err(StdError::generic_err("").to_string())
                }
                "\0\u{8}pool_ids\0\u{4}juno\0\u{3}xrpwyndex" => {
                    Err(StdError::generic_err("").to_string())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{4}junowyndex" => {
                    Ok(to_json_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{8}pool_ids\0\u{4}juno\0\u{4}wyndwyndex" => {
                    Ok(to_json_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{3}usdwyndex" => {
                    Ok(to_json_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{5}pools\0\0\0\0\0\0\0\0" => Ok(to_json_binary(&PoolMetadata::new(
                    WYNDEX,
                    abstract_core::objects::PoolType::ConstantProduct,
                    vec!["usd", "eur"],
                ))
                .unwrap()),
                _ => {
                    println!();
                    panic!("Key: {key:?} not matched in TEST_ANS mock querier");
                }
            })
            // .with_raw_handler(TEST_PROXY, |key| match key {
            //     "admin" => Ok(to_json_binary(&Some(Addr::unchecked(TEST_MANAGER))).unwrap()),
            //     _ => panic!("unexpected raw key"),
            // })
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::ACCOUNT_MODULES,
                (
                    "abstract:cw-staking",
                    Addr::unchecked(TEST_CW_STAKING_MODULE),
                ),
            )
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::ACCOUNT_MODULES,
                ("abstract:dex", Addr::unchecked(TEST_DEX)),
            )
    }

    // same as app_base_mock_querier but there is unbonding period for tokens
    pub fn app_base_mock_querier_with_unbonding_period() -> MockQuerierBuilder {
        let abstract_env = AbstractMockQuerierBuilder::default().account(
            TEST_MANAGER,
            TEST_PROXY,
            TEST_ACCOUNT_ID,
        );
        abstract_env
            .builder()
            .with_smart_handler(TEST_CW_STAKING_MODULE, |msg| {
                match from_json(msg).unwrap() {
                    abstract_cw_staking::msg::QueryMsg::Module(StakingQueryMsg::Info {
                        provider: _,
                        staking_tokens: _,
                    }) => {
                        let resp = StakingInfoResponse {
                            infos: vec![StakingInfo {
                                staking_target: StakingTarget::Contract(Addr::unchecked(
                                    "staking_addr",
                                )),
                                staking_token: AssetInfo::cw20(Addr::unchecked("usd_eur_lp")),
                                unbonding_periods: Some(vec![
                                    SHORT_UNBONDING_PERIOD,
                                    LONG_UNBONDING_PERIOD,
                                ]),
                                max_claims: Some(MAX_CLAIMS_PER_ADDRESS),
                            }],
                        };
                        Ok(to_json_binary(&resp).unwrap())
                    }
                    _ => panic!("unexpected message"),
                }
            })
            .with_smart_handler(TEST_DEX, |msg| match from_json(msg).unwrap() {
                abstract_dex_adapter::msg::QueryMsg::Module(DexQueryMsg::SimulateSwap {
                    offer_asset: _,
                    ask_asset: _,
                    dex: _,
                }) => {
                    let resp = "hello darkness my old friend";
                    Ok(to_json_binary(&resp).unwrap())
                }
                _ => panic!("unexpected message"),
            })
            .with_raw_handler(TEST_ANS_HOST, |key| match key {
                "\0\u{6}assetseur" => Ok(to_json_binary(&AssetInfo::Native("eur".into())).unwrap()),
                "\0\nrev_assets\0\u{7}native:eur" => {
                    Ok(to_json_binary(&"eur".to_string()).unwrap())
                }
                "\0\nrev_assets\0\u{7}native:juno" => {
                    Ok(to_json_binary(&"juno".to_string()).unwrap())
                }
                "\0\u{6}assetsusd" => Ok(to_json_binary(&AssetInfo::Native("usd".into())).unwrap()),
                "\0\u{6}assetswyndex/eur,usd" => {
                    Ok(to_json_binary(&AssetInfo::cw20(Addr::unchecked("usd_eur_lp"))).unwrap())
                }
                "\0\tcontracts\0\twyndexstaking/wyndex/eur,usd" => {
                    Ok(to_json_binary(&Addr::unchecked("staking_addr")).unwrap())
                }
                "\0\u{8}pool_ids\0\u{3}eur\0\u{3}usdwyndex" => {
                    Ok(to_json_binary(&vec![PoolReference {
                        unique_id: 0.into(),
                        pool_address: abstract_core::objects::pool_id::PoolAddressBase::Contract(
                            Addr::unchecked(TEST_POOL_ADDR),
                        ),
                    }])
                    .unwrap())
                }
                "\0\u{5}pools\0\0\0\0\0\0\0\0" => Ok(to_json_binary(&PoolMetadata::new(
                    WYNDEX,
                    abstract_core::objects::PoolType::ConstantProduct,
                    vec!["usd", "eur"],
                ))
                .unwrap()),
                _ => {
                    println!();
                    panic!("Key: {key:?} not matched in TEST_ANS mock querier");
                }
            })
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::ACCOUNT_MODULES,
                (
                    "abstract:cw-staking",
                    Addr::unchecked(TEST_CW_STAKING_MODULE),
                ),
            )
            .with_contract_map_entry(
                TEST_MANAGER,
                abstract_core::manager::state::ACCOUNT_MODULES,
                ("abstract:dex", Addr::unchecked(TEST_DEX)),
            )
    }

    pub fn app_init(is_unbonding_period_enabled: bool, vault_token_is_cw20: bool) -> MockDeps {
        let mut deps = mock_dependencies();
        let info = mock_info(TEST_MODULE_FACTORY, &[]);

        let bonding_data: Option<BondingData>;
        if is_unbonding_period_enabled {
            deps.querier = app_base_mock_querier_with_unbonding_period().build();
            bonding_data = Some(BondingData {
                unbonding_period: SHORT_UNBONDING_PERIOD,
                max_claims_per_address: Some(MAX_CLAIMS_PER_ADDRESS),
            });
        } else {
            deps.querier = app_base_mock_querier().build();
            bonding_data = None;
        }

        AUTOCOMPOUNDER_APP
            .instantiate(
                deps.as_mut(),
                mock_env(),
                info,
                abstract_core::app::InstantiateMsg {
                    module: crate::msg::AutocompounderInstantiateMsg {
                        code_id: if vault_token_is_cw20 { Some(1) } else { None },
                        commission_addr: COMMISSION_RECEIVER.to_string(),
                        deposit_fees: Decimal::percent(3),
                        dex: WYNDEX.to_string(),
                        performance_fees: Decimal::percent(3),
                        pool_assets: vec!["eur".into(), "usd".into()],
                        withdrawal_fees: Decimal::percent(3),
                        bonding_data,
                        max_swap_spread: None,
                    },
                    base: abstract_core::app::BaseInstantiateMsg {
                        ans_host_address: TEST_ANS_HOST.to_string(),
                        version_control_address: TEST_VERSION_CONTROL.to_string(),
                        account_base: AccountBase {
                            manager: Addr::unchecked(TEST_MANAGER),
                            proxy: Addr::unchecked(TEST_PROXY),
                        },
                    },
                },
            )
            .unwrap();

        deps
    }
}
