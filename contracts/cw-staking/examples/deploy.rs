use abstract_boot::{AnsHost, Deployment, DexApi, ModuleDeployer, VCExecFns, VCQueryFns, VersionControl};
use boot_core::networks::UNI_5;
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::{Addr, Empty};
use semver::Version;
use std::sync::Arc;
use abstract_sdk::os::objects::module::{Module, ModuleInfo, ModuleVersion};
use tokio::runtime::Runtime;
use forty_two::cw_staking::CW_STAKING;
use forty_two_boot::cw_staking::CwStakingApi;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_cw_staking(args: Arguments) -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();
    let network = UNI_5;

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let abstract_version: Version = std::env::var("ABSTRACT_VERSION").expect("Missing ABSTRACT_VERSION").parse().unwrap();
    let mut deployer = ModuleDeployer::load_from_version_control(
        chain.clone(),
        &abstract_version,
        &Addr::unchecked(std::env::var("VERSION_CONTROL").expect("VERSION_CONTROL not set")),
    )?;

    if args.prev_version.is_some() {
        let Module {
            info,
            reference
        } = deployer.version_control.module(ModuleInfo::from_id(CW_STAKING, ModuleVersion::from(args.prev_version.unwrap()))?)?.module;

        let new_info = ModuleInfo {
            version: ModuleVersion::from(CONTRACT_VERSION),
            ..info
        };
        deployer.version_control.add_modules(vec![(
            new_info,
            reference
        )])?;
    } else {
        let mut cw_staking = CwStakingApi::new(CW_STAKING, chain.clone());

        deployer.deploy_api(cw_staking.as_instance_mut(), version, Empty {})?;
    }

    Ok(())
}

use clap::Parser;
#[derive(Parser,Default,Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Use a previously deployed verison
    #[arg(short, long)]
    prev_version: Option<String>,
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    deploy_cw_staking(args)
}
