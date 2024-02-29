use abstract_core::{ans_host::QueryMsgFns, objects::AccountId};
use cw_orch::daemon::DaemonBuilder;
use cw_orch::prelude::Deploy;
use std::env;
use std::sync::Arc;

use abstract_interface::{Abstract, AbstractAccount, ManagerQueryFns};

use clap::Parser;
use cw_orch::daemon::networks::parse_network;

use log::info;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const _MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn init_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (main_account_id, dex, base_pair_asset, _cw20_code_id) = match args.network_id.as_str() {
        "uni-6" => (None, "wyndex", "juno>junox", Some(4012)),
        "juno-1" => (None, "wyndex", "juno>juno", Some(1)),
        "pion-1" => (None, "astroport", "neutron>astro", Some(188)),
        "neutron-1" => (None, "astroport", "neutron>astro", Some(180)),
        "pisco-1" => (None, "astroport", "terra2>luna", Some(83)),
        "phoenix-1" => (None, "astroport", "terra2>luna", Some(69)),
        "osmo-test-5" => (Some(2), "osmosis5", "osmosis5>osmo", Some(1)),
        "harpoon-4" => (Some(2), "kujira", "kujira>kuji", None),
        _ => panic!("Unknown network id: {}", args.network_id),
    };

    info!("Using dex: {} and base: {}", dex, base_pair_asset);

    // Setup the environment
    let network = parse_network(&args.network_id).unwrap();

    // TODO: make grpc url dynamic by removing this line once cw-orch gets updated
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;

    let abstr = Abstract::load_from(chain.clone())?;
    let main_account = if let Some(account_id) = main_account_id {
        AbstractAccount::new(&abstr, AccountId::local(account_id))
    } else {
        panic!("Not implemented yet");
    };

    info!(
        "Created account: {:?}",
        main_account
            .manager
            .sub_account_ids(None, None)?
            .sub_accounts
            .last()
    );

    let asset_list = abstr.ans_host.asset_info_list(None, None, None)?;

    info!("Asset list: {:?}", asset_list);

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Network to deploy on
    #[arg(short, long)]
    network_id: String,
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    if let Err(ref err) = init_vault(args) {
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
