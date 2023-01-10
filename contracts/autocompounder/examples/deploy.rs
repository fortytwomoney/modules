use abstract_boot::{AnsHost, Deployment, DexApi, ModuleDeployer, VCExecFns, VersionControl};
use boot_core::networks::UNI_5;
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::{Addr, Empty};
use semver::Version;
use std::sync::Arc;
use abstract_sdk::os::objects::module::{ModuleInfo, ModuleVersion};
use abstract_sdk::os::objects::module_reference::ModuleReference;
use tokio::runtime::Runtime;
use forty_two::autocompounder::AUTOCOMPOUNDER;
use forty_two::cw_staking::CW_STAKING;
use forty_two_boot::autocompounder::AutocompounderApp;
use forty_two_boot::cw_staking::CwStakingApi;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_autocompounder() -> anyhow::Result<()> {
    // let version: Version = CONTRACT_VERSION.parse().unwrap();
    let network = UNI_5;

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let mut version_control = VersionControl::load(
        chain.clone(),
        &Addr::unchecked("juno1q8tuzav8y6aawhc4sddqnwj6q4gdvn7lyk3m9ks4uw69xp37j83ql3ck2q"),
    );

    let mut autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain.clone());
    // autocompounder.upload()?;

    // version_control.register_apps(vec![autocompounder.as_instance()], &version)?;

    // // Remove beforehand
    // version_control.remove_module(ModuleInfo {
    //     name: "autocompounder".into(),
    //     provider: "4t2".into(),
    //     version: ModuleVersion::from(CONTRACT_VERSION)
    // })?;

    version_control.add_modules(vec![(ModuleInfo {
        name: "autocompounder".into(),
        provider: "4t2".into(),
        version: ModuleVersion::from(CONTRACT_VERSION)
    }, ModuleReference::App(autocompounder.code_id()?))])?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    deploy_autocompounder()
}
