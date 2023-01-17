use abstract_boot::{Abstract, OSFactory};
use abstract_boot::{DexApi, OS};
use abstract_os::{api::InstantiateMsg, objects::gov_type::GovernanceDetails, EXCHANGE};
use boot_core::{
    prelude::{BootInstantiate, BootUpload, ContractInstance},
    Mock,
};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::ContractWrapper;
use forty_two::{autocompounder::AUTOCOMPOUNDER, cw_staking::CW_STAKING};
use forty_two_boot::autocompounder::AutocompounderApp;

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange(
    chain: Mock,
    deployment: &Abstract<Mock>,
    version: Option<String>,
) -> anyhow::Result<DexApi<Mock>> {
    let mut exchange = DexApi::new(EXCHANGE, chain.clone());
    exchange
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::dex::contract::execute,
            ::dex::contract::instantiate,
            ::dex::contract::query,
        )));
    exchange.upload()?;
    exchange.instantiate(
        &InstantiateMsg {
            app: Empty {},
            base: abstract_os::api::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version: semver::Version = version
        .map(|s| s.parse().unwrap())
        .unwrap_or(deployment.version.clone());

    deployment
        .version_control
        .register_apis(vec![exchange.as_instance()], &version)?;
    Ok(exchange)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_staking(
    chain: Mock,
    deployment: &Abstract<Mock>,
    version: Option<String>,
) -> anyhow::Result<forty_two_boot::cw_staking::CwStakingApi<Mock>> {
    let mut staking = forty_two_boot::cw_staking::CwStakingApi::new(CW_STAKING, chain.clone());
    staking
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::cw_staking::contract::execute,
            ::cw_staking::contract::instantiate,
            ::cw_staking::contract::query,
        )));
    staking.upload()?;
    staking.instantiate(
        &InstantiateMsg {
            app: Empty {},
            base: abstract_os::api::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version: semver::Version = version
        .map(|s| s.parse().unwrap())
        .unwrap_or(deployment.version.clone());

    deployment
        .version_control
        .register_apis(vec![staking.as_instance()], &version)?;
    Ok(staking)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_auto_compounder(
    chain: Mock,
    deployment: &Abstract<Mock>,
    _version: Option<String>,
) -> anyhow::Result<forty_two_boot::autocompounder::AutocompounderApp<Mock>> {
    let mut auto_compounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain.clone());

    auto_compounder.as_instance_mut().set_mock(Box::new(
        ContractWrapper::new_with_empty(
            autocompounder::contract::execute,
            autocompounder::contract::instantiate,
            autocompounder::contract::query,
        )
        .with_reply_empty(::autocompounder::contract::reply),
    ));

    // upload and register autocompounder
    auto_compounder.upload().unwrap();

    deployment
        .version_control
        .register_apps(vec![auto_compounder.as_instance()], &deployment.version)
        .unwrap();

    Ok(auto_compounder)
}
