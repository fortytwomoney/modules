use abstract_interface::AppDeployer;
use cw_orch::daemon::networks::PION_1;
use cw_orch::daemon::ChainInfo;

use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::AUTOCOMPOUNDER;
use cw_orch::prelude::*;
use std::env;
use std::sync::Arc;
use tokio::runtime::Runtime;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_autocompounder(
    network: ChainInfo,
    _autocompounder_code_id: Option<u64>,
) -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;

    let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

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

    let network = PION_1;

    deploy_autocompounder(network, args.code_id)
}
