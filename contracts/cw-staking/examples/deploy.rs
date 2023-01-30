use abstract_boot::{ModuleDeployer, VCExecFns, VCQueryFns};
use abstract_sdk::os::objects::module::{Module, ModuleInfo, ModuleVersion};
use boot_core::networks::{NetworkInfo, UNI_5};
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::{Addr, Empty};
use forty_two::cw_staking::CW_STAKING;
use forty_two_boot::cw_staking::CwStakingApi;
use semver::Version;
use std::sync::Arc;
use tokio::runtime::Runtime;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_cw_staking(_network: NetworkInfo, prev_version: Option<String>) -> anyhow::Result<()> {
    let module_version: Version = CONTRACT_VERSION.parse().unwrap();
    let network = UNI_5;

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let abstract_version: Version = std::env::var("ABSTRACT_VERSION")
        .expect("Missing ABSTRACT_VERSION")
        .parse()
        .unwrap();
    let deployer = ModuleDeployer::load_from_version_control(
        chain.clone(),
        &abstract_version,
        &Addr::unchecked(std::env::var("VERSION_CONTROL").expect("VERSION_CONTROL not set")),
    )?;

    if let Some(prev_version) = prev_version {
        let Module { info, reference } = deployer
            .version_control
            .module(ModuleInfo::from_id(
                CW_STAKING,
                ModuleVersion::from(prev_version),
            )?)?
            .module;

        let new_info = ModuleInfo {
            version: ModuleVersion::from(CONTRACT_VERSION),
            ..info
        };
        deployer
            .version_control
            .add_modules(vec![(new_info, reference)])?;
    } else {
        log::info!("Uploading Cw staking");
        // Upload and deploy with the version
        let mut cw_staking = CwStakingApi::new(CW_STAKING, chain);

        deployer.deploy_api(cw_staking.as_instance_mut(), module_version, Empty {})?;
    }

    Ok(())
}

use clap::Parser;
use forty_two_boot::parse_network;

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Use a previously deployed version instead of uploading the new one
    #[arg(short, long)]
    prev_version: Option<String>,
    #[arg(short, long)]
    network_id: String,
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let Arguments {
        network_id,
        prev_version,
    } = Arguments::parse();

    let network = parse_network(&network_id);

    deploy_cw_staking(network, prev_version)
}
