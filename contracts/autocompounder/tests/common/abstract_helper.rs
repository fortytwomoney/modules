use std::str::FromStr;

use abstract_cw_staking::{interface::CwStakingAdapter, CW_STAKING_ADAPTER_ID};
use abstract_dex_adapter::msg::DexInstantiateMsg;
use abstract_dex_adapter::{interface::DexAdapter, DEX_ADAPTER_ID};
use abstract_interface::{Abstract, AbstractInterfaceError};
use abstract_sdk::core as abstract_core;
use abstract_sdk::core::adapter::InstantiateMsg;
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::AUTOCOMPOUNDER_ID;
use cosmwasm_std::{Decimal, Empty};
use cw_orch::prelude::*;

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange(
    chain: Mock,
    deployment: &Abstract<Mock>,
    version: Option<String>,
) -> Result<DexAdapter<Mock>, AbstractInterfaceError> {
    let exchange = DexAdapter::new(DEX_ADAPTER_ID, chain);

    exchange.upload()?;
    exchange.instantiate(
        &InstantiateMsg {
            module: DexInstantiateMsg {
                swap_fee: Decimal::from_str("0.003")?,
                recipient_account: 0,
            },
            base: abstract_core::adapter::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version =
        version.unwrap_or_else(|| abstract_dex_adapter::contract::CONTRACT_VERSION.to_string());

    deployment
        .version_control
        .register_adapters(vec![(exchange.as_instance(), version)])?;
    Ok(exchange)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_staking(
    chain: Mock,
    deployment: &Abstract<Mock>,
    version: Option<String>,
) -> Result<CwStakingAdapter<Mock>, AbstractInterfaceError> {
    let staking = CwStakingAdapter::new(CW_STAKING_ADAPTER_ID, chain);

    staking.upload()?;
    staking.instantiate(
        &InstantiateMsg {
            module: Empty {},
            base: abstract_core::adapter::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version =
        version.unwrap_or_else(|| abstract_cw_staking::contract::CONTRACT_VERSION.to_string());

    deployment
        .version_control
        .register_adapters(vec![(staking.as_instance(), version)])?;
    Ok(staking)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_auto_compounder(
    chain: Mock,
    deployment: &Abstract<Mock>,
    _version: Option<String>,
) -> Result<autocompounder::interface::AutocompounderApp<Mock>, AbstractInterfaceError> {
    let auto_compounder = AutocompounderApp::new(AUTOCOMPOUNDER_ID, chain);

    // upload and register autocompounder
    auto_compounder.upload().unwrap();

    deployment
        .version_control
        .register_apps(vec![(
            auto_compounder.as_instance(),
            autocompounder::contract::MODULE_VERSION.to_string(),
        )])
        .unwrap();

    Ok(auto_compounder)
}
