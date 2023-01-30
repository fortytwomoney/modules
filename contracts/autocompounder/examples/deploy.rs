use abstract_boot::VersionControl;
use boot_core::networks::{NetworkInfo};
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::Addr;
use forty_two::autocompounder::AUTOCOMPOUNDER;
use forty_two_boot::autocompounder::AutocompounderApp;
use std::env;
use std::sync::Arc;
use tokio::runtime::Runtime;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_autocompounder(
    network: NetworkInfo,
    autocompounder_code_id: Option<u64>,
) -> anyhow::Result<()> {
    // let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let version_control = VersionControl::load(
        chain.clone(),
        &Addr::unchecked(std::env::var("VERSION_CONTROL").expect("VERSION_CONTROL not set")),
    );

    let mut autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

    if let Some(code_id) = autocompounder_code_id {
        autocompounder.set_code_id(code_id);
    } else {
        // panic!("No code id provided, and upload is broken");
        autocompounder.upload()?;
    }

    // version_control.register_apps(vec![autocompounder.as_instance()], &version)?;

    // // Remove beforehand
    // version_control.remove_module(ModuleInfo {
    //     name: "autocompounder".into(),
    //     provider: "4t2".into(),
    //     version: ModuleVersion::from(CONTRACT_VERSION)
    // })?;

    let version = CONTRACT_VERSION.parse().unwrap();
    version_control.register_apps(vec![autocompounder.as_instance()], &version)?;

    Ok(())
}

use clap::Parser;
#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Code ID of the already uploaded contract
    #[arg(short, long)]
    code_id: Option<u64>,
    #[arg(short, long)]
    network_id: String,
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    let network = forty_two_boot::parse_network(&args.network_id);

    deploy_autocompounder(network, args.code_id)
}
