use abstract_client::AbstractClient;
use abstract_core::objects::AccountId;
use anyhow::Ok;
use autocompounder::interface::{AutocompounderApp, Vault};

use cw_orch::daemon::DaemonBuilder;

use cw_orch::prelude::*;
use semver::Version;
use std::env;
use std::sync::Arc;

use abstract_interface::{AppDeployer, DeployStrategy, ManagerQueryFns};

use clap::Parser;

use cw_orch::daemon::networks::parse_network;

use autocompounder::msg::AUTOCOMPOUNDER;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn migrate_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let network = parse_network(&args.network_id).unwrap();
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;

    let abstract_client = AbstractClient::new(chain.clone())?;
    let account_id = AccountId::local(args.account_id);
    let account = abstract_client.account_from(account_id)?;

    let mut vault = Vault::new(account.as_ref())?;

    let versions = vault
        .account
        .manager
        .module_versions(vec![AUTOCOMPOUNDER.to_string()])?
        .versions;
    let current_version: Version = versions[0].version.clone().parse()?;

    let new_version: Version = CONTRACT_VERSION.parse().unwrap();

    if current_version >= new_version {
        return Err(anyhow::anyhow!(
            "Already latest version {} >= {}",
            current_version,
            new_version
        ));
    }
    println!(
        "Migrating vault {} from version {} to {}",
        args.account_id, current_version, new_version
    );

    let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);
    if !vault.autocompounder.latest_is_uploaded()? {
        println!(
            "Wasm hash on chain is outdated. Uploading and registering new version... {}",
            new_version
        );

        autocompounder
            .deploy(new_version, DeployStrategy::Error)
            .map_err(|e| {
                println!(
                "Error deploying. If its a version error, try do switch this part of the code to 
            manual uploading and version registration, as that surpasses the version control. {:?}",
                e
            );
                e
            })?;

        // // in case the .deploy function complains about versioning, use this:
        // // WARNING: This will overwrite the currently registered code for the version if it exists
        // autocompounder.upload()?;
        //     abstr
        // .version_control
        // .register_apps(vec![(autocompounder.as_instance(), version.to_string())])?;
    } else {
        println!("Vault is already uploaded. {}", new_version);
    }

    println!("updating vault dependencies...");

    vault.update()?;

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Account ID of the vault to migrate
    #[arg(short, long)]
    account_id: u32,
    /// Network to deploy on
    #[arg(short, long)]
    network_id: String,
    // #[arg(short, long)]
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    if let Err(ref err) = migrate_vault(args) {
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
