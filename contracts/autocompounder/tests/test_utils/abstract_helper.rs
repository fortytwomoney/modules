use std::str::FromStr;

use boot_core::*;
use abstract_boot::{Abstract, AbstractBootError};
use abstract_cw_staking_api::{boot::CwStakingApi, CW_STAKING};
use abstract_dex_api::msg::DexInstantiateMsg;
use abstract_dex_api::{boot::DexApi, EXCHANGE};
use abstract_sdk::core as abstract_core;
use abstract_sdk::core::api::InstantiateMsg;
use cosmwasm_std::{Decimal, Empty};
use cw_multi_test::ContractWrapper;
use autocompounder::msg::AUTOCOMPOUNDER;
use autocompounder::autocompounder::AutocompounderApp;

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange(
    chain: Mock,
    deployment: &Abstract<Mock>,
    version: Option<String>,
) -> Result<DexApi<Mock>, AbstractBootError> {
    let mut exchange = DexApi::new(EXCHANGE, chain);
    exchange
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::abstract_dex_api::contract::execute,
            ::abstract_dex_api::contract::instantiate,
            ::abstract_dex_api::contract::query,
        )));
    exchange.upload()?;
    exchange.instantiate(
        &InstantiateMsg {
            module: DexInstantiateMsg {
                swap_fee: Decimal::from_str("0.003")?,
                recipient_os: 0,
            },
            base: abstract_core::api::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version: semver::Version = version
        .map(|s| s.parse().unwrap())
        .unwrap_or_else(|| deployment.version.clone());

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
) -> Result<CwStakingApi<Mock>, AbstractBootError> {
    let mut staking = CwStakingApi::new(CW_STAKING, chain);
    staking
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::abstract_cw_staking_api::contract::execute,
            ::abstract_cw_staking_api::contract::instantiate,
            ::abstract_cw_staking_api::contract::query,
        )));
    staking.upload()?;
    staking.instantiate(
        &InstantiateMsg {
            module: Empty {},
            base: abstract_core::api::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version: semver::Version = version
        .map(|s| s.parse().unwrap())
        .unwrap_or_else(|| deployment.version.clone());

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
) -> Result<autocompounder::autocompounder::AutocompounderApp<Mock>, AbstractBootError> {
    let mut auto_compounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

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
