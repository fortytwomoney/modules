use abstract_boot::boot_core;
use abstract_boot::boot_core::{BootUpload, ContractInstance};
use abstract_boot::VersionControl;
use boot_core::networks::{parse_network, NetworkInfo};
use boot_core::*;
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
    let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;
    let mut autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain.clone());

    autocompounder.deploy(version)?;

    Ok(())
}

use clap::Parser;
use semver::Version;

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

    let network = parse_network(&args.network_id);

    deploy_autocompounder(network, args.code_id)
}
