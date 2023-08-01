use abstract_interface::AppDeployer;
use cw_orch::daemon::networks::parse_network;
use std::sync::Arc;

use cw_orch::daemon::ChainInfo;
use cw_orch::prelude::*;
use semver::Version;

use clap::Parser;
use fee_collector::contract::interface::FeeCollectorInterface;
use fee_collector::msg::FEE_COLLECTOR;
use tokio::runtime::Runtime;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_etf(network: ChainInfo) -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;
    let etf = FeeCollectorInterface::new(FEE_COLLECTOR, chain);

    etf.deploy(version)?;
    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Network Id to deploy on
    #[arg(short, long)]
    network_id: String,
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    let network = parse_network(&args.network_id);

    deploy_etf(network)
}
