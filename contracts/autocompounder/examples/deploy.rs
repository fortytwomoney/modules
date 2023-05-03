use abstract_boot::AppDeployer;

use autocompounder::boot::AutocompounderApp;
use autocompounder::msg::AUTOCOMPOUNDER;
use boot_core;
use boot_core::networks::juno::JUNO_CHAIN;
use boot_core::networks::{parse_network, NetworkInfo, NetworkKind};
use boot_core::*;
use std::env;
use std::sync::Arc;
use tokio::runtime::Runtime;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const JUNO_1: NetworkInfo = NetworkInfo {
    kind: NetworkKind::Mainnet,
    id: "juno-1",
    gas_denom: "ujuno",
    gas_price: 0.0025,
    grpc_urls: &["http://juno-grpc.polkachu.com:12690"],
    chain_info: JUNO_CHAIN,
    lcd_url: None,
    fcd_url: None,
};

fn deploy_autocompounder(
    _network: NetworkInfo,
    _autocompounder_code_id: Option<u64>,
) -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(JUNO_1).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let mut autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

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
