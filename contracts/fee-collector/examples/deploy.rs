use abstract_interface::{AppDeployer, DeployStrategy};
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

fn deploy_fc(network: ChainInfo) -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;
    let fee_collector = FeeCollectorInterface::new(FEE_COLLECTOR, chain);

    fee_collector.deploy(version, DeployStrategy::Try)?;

    // let abstr = Abstract::load_from(fee_collector.get_chain().to_owned())?;
    // // check for existing version
    // self.upload()?;
    // fee_collector.set_code_id(1403);

    // abstr
    //     .version_control
    //     .register_apps(vec![(fee_collector.as_instance(), version.to_string())])?;

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

    let network = parse_network(&args.network_id).unwrap();

    deploy_fc(network)?;

    // if let Err(ref err) = deploy_fc(network) {
    //     if let Some(backtrace) = err.backtrace() {
    //         log::debug!("backtrace: {:?}", backtrace);
    //     }
    // };

    Ok(())
}
