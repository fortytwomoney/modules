use abstract_boot::{boot_core::DaemonOptionsBuilder, VersionControl};
use abstract_core::objects::{AnsAsset, PoolMetadata};
use autocompounder::{
    boot::Vault,
    msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns},
    state::Config,
};
use boot_core::{instantiate_daemon_env, networks::parse_network};
use clap::Parser;
use cosmwasm_std::Addr;
use log::info;
use speculoos::prelude::*;
use std::sync::Arc;

fn test_compound(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (dex, base_pair_asset) = match args.network_id.as_str() {
        "uni-5" => ("junoswap", "junox"),
        "juno-1" => ("junoswap", "juno"),
        "pisco-1" => ("astroport", "terra2>luna"),
        _ => panic!("Unknown network id: {}", args.network_id),
    };

    info!("Using dex: {} and base: {}", dex, base_pair_asset);
    let network = parse_network(&args.network_id);
    let daemon_options = DaemonOptionsBuilder::default().network(network).build()?;
    // Setup the environment
    let (sender, chain) = instantiate_daemon_env(&rt, daemon_options)?;

    // Set version control address
    let _vc = VersionControl::load(
        chain.clone(),
        &Addr::unchecked(std::env::var("VERSION_CONTROL").expect("VERSION_CONTROL not set")),
    );

    let mut vault: Vault<_> = Vault::new(chain, Some(args.vault_id))?;

    // Update the modules in the vault
    vault.update()?;

    let autocompounder = vault.autocompounder;

    // TODO: get the exchange rate
    let Config {
        pool_data: PoolMetadata {
            assets: _pool_assets,
            ..
        },
        // liquidity_token,
        ..
    } = AutocompounderQueryMsgFns::config(&autocompounder)?;

    let lp_balance_before_deposit = autocompounder.balance(sender.to_string())?;
    info!("LP balance before: {}", lp_balance_before_deposit);

    // , AnsAsset::new("terra2>luna", 10u128)
    autocompounder.deposit(vec![AnsAsset::new("terra2>astro", 6942u128)], None, &[])?;

    let lp_balance_after_deposit = autocompounder.balance(sender.to_string())?;
    info!("LP balance after: {}", lp_balance_after_deposit);

    assert_that!(lp_balance_after_deposit).is_greater_than(lp_balance_before_deposit);

    autocompounder.compound()?;

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Vault id to test
    #[arg(short, long)]
    vault_id: u32,
    #[arg(short, long)]
    network_id: String,
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    if let Err(ref err) = test_compound(args) {
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
