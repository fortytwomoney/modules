use abstract_interface::{AppDeployer, DeployStrategy};
use cw_orch::daemon::networks::osmosis::OSMO_NETWORK;
use cw_orch::daemon::{ChainInfo, ChainKind};

use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::AUTOCOMPOUNDER_ID;

use cw_orch::prelude::networks::parse_network;
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

    let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER_ID, chain);

    autocompounder.deploy(version, DeployStrategy::Error)?;

    // // This might be still useful at some point for instantiation fees
    // let update = abstr.version_control.update_module_configuration(
    //     AUTOCOMPOUNDER.to_string(),
    //     Namespace::new("4t2")?,
    //     abstract_core::version_control::UpdateModule::Versioned {
    //         version: MODULE_VERSION.to_string(),
    //         metadata: None,
    //         monetization: None,
    //         instantiation_funds: instantiation_funds.clone(),
    //     },
    // )?;

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

    pub const OSMOSIS_1: ChainInfo = ChainInfo {
        kind: ChainKind::Mainnet,
        chain_id: "osmosis-1",
        gas_denom: "uosmo",
        gas_price: 0.025,
        grpc_urls: &["http://grpc.osmosis.zone:9090"],
        network_info: OSMO_NETWORK,
        lcd_url: None,
        fcd_url: None,
    };

    let network = if &args.network_id == "osmosis-1" {
        OSMOSIS_1
    } else {
        parse_network(&args.network_id).unwrap()
    };
    deploy_autocompounder(network, args.code_id)
}
