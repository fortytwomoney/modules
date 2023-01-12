use std::env;
use abstract_boot::{VCExecFns, VersionControl};
use boot_core::networks::{NetworkInfo, UNI_5};
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::{Addr};

use std::sync::Arc;
use abstract_sdk::os::objects::module::{ModuleInfo, ModuleVersion};
use abstract_sdk::os::objects::module_reference::ModuleReference;
use tokio::runtime::Runtime;
use forty_two::autocompounder::AUTOCOMPOUNDER;

use forty_two_boot::autocompounder::AutocompounderApp;


const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const NETWORK: NetworkInfo = UNI_5;


fn deploy_autocompounder(args: Arguments) -> anyhow::Result<()> {
    // let version: Version = CONTRACT_VERSION.parse().unwrap();

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(NETWORK).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let version_control = VersionControl::load(
        chain.clone(),
        &Addr::unchecked(std::env::var("VERSION_CONTROL").expect("VERSION_CONTROL not set")),
    );

    let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

    if args.code_id.is_none() {
        panic!("No code id provided, and upload is broken");
        // autocompounder.upload()?;
    } else {
        autocompounder.set_code_id(args.code_id.unwrap());
    }

    // version_control.register_apps(vec![autocompounder.as_instance()], &version)?;

    // // Remove beforehand
    // version_control.remove_module(ModuleInfo {
    //     name: "autocompounder".into(),
    //     provider: "4t2".into(),
    //     version: ModuleVersion::from(CONTRACT_VERSION)
    // })?;

    version_control.add_modules(vec![(ModuleInfo {
        name: "autocompounder".into(),
        provider: "4t2".into(),
        version: ModuleVersion::from(CONTRACT_VERSION)
    }, ModuleReference::App(autocompounder.code_id()?))])?;

    Ok(())
}

use clap::Parser;
#[derive(Parser,Default,Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// COde ID of the already uploaded contract
    #[arg(short, long)]
    code_id: Option<u64>,
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    deploy_autocompounder(args)

}
