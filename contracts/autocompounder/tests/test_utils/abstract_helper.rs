use std::str::FromStr;

use abstract_boot::boot_core::*;
use abstract_boot::{Abstract, AbstractBootError};
use abstract_sdk::core as abstract_core;
use abstract_sdk::core::api::InstantiateMsg;
use cosmwasm_std::{Decimal, Empty};
use cw_multi_test::ContractWrapper;
use cw_staking::{boot::CwStakingApi, CW_STAKING};
use dex::msg::DexInstantiateMsg;
use dex::{boot::DexApi, EXCHANGE};
use forty_two::autocompounder::AUTOCOMPOUNDER;
use forty_two_boot::autocompounder::AutocompounderApp;

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
            ::dex::contract::execute,
            ::dex::contract::instantiate,
            ::dex::contract::query,
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
            ::cw_staking::contract::execute,
            ::cw_staking::contract::instantiate,
            ::cw_staking::contract::query,
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
) -> Result<forty_two_boot::autocompounder::AutocompounderApp<Mock>, AbstractBootError> {
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
