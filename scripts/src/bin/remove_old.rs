use abstract_boot::{
    boot_core::{networks, networks::NetworkInfo, DaemonOptionsBuilder},
    VCExecFns, VersionControl,
};
use abstract_core::objects::module::{ModuleInfo, ModuleVersion};
use boot_core::instantiate_daemon_env;
use cosmwasm_std::Addr;
use std::env;
use std::sync::Arc;

const NETWORK: NetworkInfo = networks::UNI_6;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const _MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn deploy_api() -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let daemon_options = DaemonOptionsBuilder::default().network(NETWORK).build()?;

    // Setup the environment
    let (_sender, chain) = instantiate_daemon_env(&rt, daemon_options)?;

    // Load Abstract Version Control
    let version_control_address: String =
        env::var("VERSION_CONTROL").expect("VERSION_CONTROL_ADDRESS must be set");

    let version_control = VersionControl::load(chain, &Addr::unchecked(version_control_address));

    let old_versions = vec!["0.1.0", "0.1.1", "0.1.2", "0.1.3", "0.1.4", "0.1.5"];

    for version in old_versions {
        version_control.remove_module(ModuleInfo {
            name: "autocompounder".to_string(),
            namespace: "4t2".into(),
            version: ModuleVersion::from(version),
        })?;

        version_control.remove_module(ModuleInfo {
            name: "cw-staking".to_string(),
            namespace: "4t2".into(),
            version: ModuleVersion::from(version),
        })?;
    }

    Ok(())
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    if let Err(ref err) = deploy_api() {
        log::error!("{}", err);
        err.chain()
            .skip(1)
            .for_each(|cause| log::error!("because: {}", cause));

        // The backtrace is not always generated. Try to run this example
        // with `$env:RUST_BACKTRACE=1`.
        //    if let Some(backtrace) = e.backtrace() {
        //        log::debug!("backtrace: {:?}", backtrace);
        //    }

        ::std::process::exit(1);
    }
}
