use std::str::FromStr;

use abstract_cw_staking::{interface::CwStakingAdapter, CW_STAKING_ADAPTER_ID};
use abstract_dex_adapter::msg::DexInstantiateMsg;
use abstract_dex_adapter::{interface::DexAdapter, DEX_ADAPTER_ID};
use abstract_interface::{Abstract, AbstractInterfaceError, AdapterDeployer, DeployStrategy};
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::AUTOCOMPOUNDER_ID;
use cosmwasm_std::{Decimal, Empty};
use cw_orch::prelude::*;

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange<Chain: CwEnv>(
    chain: Chain,
    version: Option<String>,
) -> Result<DexAdapter<Chain>, AbstractInterfaceError> {
    let exchange = DexAdapter::new(DEX_ADAPTER_ID, chain);

    let version = version
        .unwrap_or_else(|| abstract_dex_adapter::contract::CONTRACT_VERSION.to_string())
        .parse()
        .unwrap();

    exchange.deploy(
        version,
        DexInstantiateMsg {
            swap_fee: Decimal::from_str("0.003")?,
            recipient_account: 0,
        },
        DeployStrategy::Try,
    )?;

    Ok(exchange)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_staking<Chain: CwEnv>(
    chain: Chain,
    version: Option<String>,
) -> Result<CwStakingAdapter<Chain>, AbstractInterfaceError> {
    let staking = CwStakingAdapter::new(CW_STAKING_ADAPTER_ID, chain);
    let version = version
        .unwrap_or_else(|| abstract_cw_staking::contract::CONTRACT_VERSION.to_string())
        .parse()
        .unwrap();

    staking.deploy(version, Empty {}, DeployStrategy::Try)?;

    Ok(staking)
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_auto_compounder(
    chain: MockBech32,
    deployment: &Abstract<MockBech32>,
    _version: Option<String>,
) -> Result<autocompounder::interface::AutocompounderApp<MockBech32>, AbstractInterfaceError> {
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
