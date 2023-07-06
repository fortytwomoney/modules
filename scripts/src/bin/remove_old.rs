use abstract_interface::VCExecFns;
use abstract_interface::VersionControl;
use cw_orch::daemon::{ChainInfo, DaemonBuilder};

use cw_orch::prelude::*;

use abstract_core::objects::module::{ModuleInfo, ModuleVersion};
use cosmwasm_std::Addr;
use std::env;
use std::sync::Arc;

const NETWORK: ChainInfo = networks::UNI_6;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const _MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn deploy_api() -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    // Setup the environment
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(NETWORK)
        .build()?;

    // Load Abstract Version Control
    let version_control_address: String =
        env::var("VERSION_CONTROL").expect("VERSION_CONTROL_ADDRESS must be set");

    let version_control = VersionControl::load(chain, &Addr::unchecked(version_control_address));

    let old_versions = vec!["0.1.0", "0.1.1", "0.1.2", "0.1.3", "0.1.4", "0.1.5"];

    for version in old_versions {
        let res = version_control.remove_module(ModuleInfo {
            name: "autocompounder".to_string(),
            namespace: "4t2".try_into()?,
            version: ModuleVersion::from(version),
        });

        if res.is_err() {
            println!("Error removing autocompounder version {version}");
        }

        let res = version_control.remove_module(ModuleInfo {
            name: "cw_staking".to_string(),
            namespace: "4t2".try_into()?,
            version: ModuleVersion::from(version),
        });

        if res.is_err() {
            println!("Error removing autocompounder version {version}");
        }
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
