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
    pub use abstract_os::app;
    use abstract_os::{module_factory::ContextResponse, version_control::Core, cw_staking::{CwStakingQueryMsg, StakingInfoResponse}};
    use abstract_sdk::base::InstantiateEndpoint;
    pub use cosmwasm_std::testing::*;
    use cosmwasm_std::{from_binary, to_binary, Addr, StdError, Decimal};
    use cw_asset::AssetInfo;
    use forty_two::autocompounder::BondingPeriodSelector;
    pub use speculoos::prelude::*;
    use abstract_testing::{
        MockDeps, MockQuerierBuilder, TEST_ANS_HOST, TEST_MANAGER, TEST_MODULE_FACTORY,
        TEST_MODULE_ID, TEST_PROXY, TEST_VERSION,
    };

    use crate::contract::AUTO_COMPOUNDER_APP;
    const ASTROPORT: &str = "astroport";
    const COMMISSION_RECEIVER: &str = "commission_receiver";

    // Mock Querier with a smart-query handler for the module factory
    // Because that query is performed when the App is instantiated to get the manager's address and set it as the Admin
    pub fn app_base_mock_querier() -> MockQuerierBuilder {
        MockQuerierBuilder::default().with_smart_handler(TEST_MODULE_FACTORY, |msg| {
            match from_binary(msg).unwrap() {
                abstract_os::module_factory::QueryMsg::Context {} => {
                    let resp = ContextResponse {
                        core: Some(Core {
                            manager: Addr::unchecked(TEST_MANAGER),
                            proxy: Addr::unchecked(TEST_PROXY),
                        }),
                        module: None,
                    };
                    Ok(to_binary(&resp).unwrap())
                },
                abstract_os::cw_staking::QueryMsg::App(CwStakingQueryMsg::Info { provider: _, staking_token: _ }) => {
                    let resp = StakingInfoResponse{
                        staking_contract_address: Addr::unchecked("staking_addr"),
                        staking_token: AssetInfo::cw20(Addr::unchecked("usd_eur_lp")),
                        unbonding_periods: None,
                        max_claims: None,
                    };
                    Ok(to_binary(&resp).unwrap())
                }
                _ => panic!("unexpected message"),
            }
        }).with_raw_handler(TEST_ANS_HOST, |key| {
            match key {
                "\0\u{6}assetseur" => {
                    Ok(to_binary(&AssetInfo::Native("eur".into())).unwrap())
                },
                "\0\u{6}assetsusd" => {
                    Ok(to_binary(&AssetInfo::Native("usd".into())).unwrap())
                },
                "\0\u{6}assetsastroport/eur,usd" => {
                    Ok(to_binary(&AssetInfo::cw20(Addr::unchecked("usd_eur_lp"))).unwrap())
                },
                "\0\tcontracts\0\tastroportstaking/astroport/eur,usd" => {
                    Ok(to_binary(&Addr::unchecked("staking_addr")).unwrap())
                },
                _ => {
                    println!();
                    panic!("Key: {:?} not matched in TEST_ANS mock querier", key);
                }
            }
        })
    }


    pub fn app_init() -> MockDeps {
        let mut deps = mock_dependencies();
        let info = mock_info(TEST_MODULE_FACTORY, &[]);

        deps.querier = app_base_mock_querier().build();

        AUTO_COMPOUNDER_APP.instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            abstract_os::app::InstantiateMsg {
                app: forty_two::autocompounder::AutocompounderInstantiateMsg {
                    code_id: 1,
                    commission_addr: COMMISSION_RECEIVER.to_string(),
                    deposit_fees: Decimal::percent(3),
                    dex: ASTROPORT.to_string(),
                    fee_asset: "eur".to_string(),
                    performance_fees: Decimal::percent(3),
                    pool_assets: vec!["eur".into(), "usd".into()],
                    withdrawal_fees: Decimal::percent(3),
                    preferred_bonding_period: BondingPeriodSelector::Shortest,
                },
                base: abstract_os::app::BaseInstantiateMsg {
                    ans_host_address: TEST_ANS_HOST.to_string(),
                },
            },
        ).unwrap();

        deps
    }
}
