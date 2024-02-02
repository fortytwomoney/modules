// use abstract_core::module_factory::ModuleInstallConfig;
// use cw_orch::daemon::DaemonBuilder;
// use cw_orch::deploy::Deploy;
// use cw_orch::environment::{CwEnv, TxResponse};
// use cw_orch::prelude::*;
// use fee_collector::msg::FEE_COLLECTOR;
// use std::env;
// use std::str::FromStr;
// use std::sync::Arc;

// use abstract_core::{
//     account_factory, adapter, app,
//     objects::module::ModuleInfo,
//     objects::{gov_type::GovernanceDetails, module::ModuleVersion},
//     registry::{ANS_HOST, MANAGER, PROXY},
//     ABSTRACT_EVENT_TYPE,
// };
// use abstract_interface::{Abstract, AbstractAccount, AccountFactory, Manager, Proxy};

// use clap::Parser;
// use cosmwasm_std::{Addr, Decimal, Empty};
// use cw_orch::daemon::networks::parse_network;

// use log::info;

// // To deploy the app we need to get the memory and then register it
// // We can then deploy a test Account that uses that new app

// const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

// fn create_fee_collector_account<Chain: CwEnv>(
//     factory: &AccountFactory<Chain>,
//     chain: Chain,
//     governance_details: GovernanceDetails<String>,
// ) -> Result<AbstractAccount<Chain>, CwOrchError>
// where
//     TxResponse<Chain>: IndexResponse,
// {
//     let result = factory.execute(
//         &account_factory::ExecuteMsg::CreateAccount {
//             base_asset: None,
//             governance: governance_details,
//             description: None,
//             link: None,
//             name: "4t2 Fee Collector".to_string(),
//             namespace: Some("4t2".to_string()),
//             install_modules: vec![
//                 ModuleInstallConfig {},
//                 ModuleInstallConfig {module: FEE_COLLECTOR.to_string(),version:ModuleVersion::from(MODULE_VERSION), module: todo!(), init_msg: todo!() }],

//         },
//         None,
//     )?;

//     let manager_address = &result.event_attr_value(ABSTRACT_EVENT_TYPE, "manager_address")?;
//     chain
//         .state()
//         .set_address(MANAGER, &Addr::unchecked(manager_address));
//     let proxy_address = &result.event_attr_value(ABSTRACT_EVENT_TYPE, "proxy_address")?;
//     chain
//         .state()
//         .set_address(PROXY, &Addr::unchecked(proxy_address));
//     Ok(AbstractAccount {
//         manager: Manager::new(MANAGER, chain.clone()),
//         proxy: Proxy::new(PROXY, chain),
//     })
// }

// fn init_fee_collector(args: Arguments) -> anyhow::Result<()> {
//     let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

//     let (dex, base_pair_asset, _cw20_code_id) = match args.network_id.as_str() {
//         "uni-6" => ("wyndex", "juno>junox", 4012),
//         "juno-1" => ("wyndex", "juno>juno", 1),
//         "pion-1" => ("astroport", "neutron>astro", 188),
//         "pisco-1" => ("astroport", "terra2>luna", 83),
//         "phoenix-1" => ("astroport", "terra2>luna", 69),
//         "osmo-test-5" => ("osmosis5", "osmosis5>osmo", 1),
//         _ => panic!("Unknown network id: {}", args.network_id),
//     };

//     info!("Using dex: {} and base: {}", dex, base_pair_asset);

//     // Setup the environment
//     let network = parse_network(&args.network_id).unwrap();

//     // TODO: make grpc url dynamic by removing this line once cw-orch gets updated
//     let chain = DaemonBuilder::default()
//         .handle(rt.handle())
//         .chain(network)
//         .build()?;
//     let sender = chain.sender();

//     let abstr = Abstract::load_from(chain.clone())?;

//     let account = if let Some(account_id) = args.account_id {
//         AbstractAccount::new(&abstr, Some(account_id))
//     } else {
//         create_fee_collector_account(
//             &abstr.account_factory,
//             chain,
//             GovernanceDetails::Monarchy {
//                 monarch: sender.to_string(),
//             },
//         )?
//     };

//     if !account.manager.is_module_installed("abstract:dex")? {
//         account
//             .manager
//             .install_module("abstract:dex", &Empty {}, None)?;
//     }

//     // Install fee collector
//     let new_module_version = ModuleVersion::from(MODULE_VERSION);

//     account.manager.install_module_version(
//         FEE_COLLECTOR,
//         new_module_version,
//         &app::InstantiateMsg {
//             base: app::BaseInstantiateMsg {
//                 ans_host_address: abstr
//                     .version_control
//                     .module(ModuleInfo::from_id_latest(ANS_HOST)?)?
//                     .reference
//                     .unwrap_addr()?
//                     .to_string(),
//             },
//             module: fee_collector::msg::FeeCollectorInstantiateMsg {
//                 commission_addr: args.commission_addr,
//                 fee_asset: args.fee_asset,
//                 dex: dex.to_string(),
//                 max_swap_spread: Decimal::from_str("0.05")?,
//             },
//         },
//         None,
//     )?;

//     // Register the fee_clollector as a trader on the cw-staking and the dex
//     let fee_collector = account.manager.module_info(FEE_COLLECTOR)?.unwrap().address;

//     account.manager.execute_on_module(
//         "abstract:dex",
//         adapter::ExecuteMsg::<Empty, Empty>::Base(
//             adapter::BaseExecuteMsg::UpdateAuthorizedAddresses {
//                 to_add: vec![fee_collector.to_string()],
//                 to_remove: vec![],
//             },
//         ),
//     )?;

//     Ok(())
// }

// #[derive(Parser, Default, Debug)]
// #[command(author, version, about, long_about = None)]
// struct Arguments {
//     /// Optionally provide an Account Id to turn into a vault
//     #[arg(short, long)]
//     account_id: Option<u32>,
//     /// commission address
//     #[arg(short, long)]
//     commission_addr: String,
//     /// Fee asset
//     /// e.g. juno>juno
//     #[arg(short, long)]
//     fee_asset: String,
//     /// Network id
//     #[arg(short, long)]
//     network_id: String,
//     // #[arg(short, long)]
//     // dex: String,
// }

fn main() {
    //     dotenv().ok();
    //     env_logger::init();

    //     use dotenv::dotenv;

    //     let args = Arguments::parse();

    //     if let Err(ref err) = init_fee_collector(args) {
    //         log::error!("{}", err);
    //         err.chain()
    //             .skip(1)
    //             .for_each(|cause| log::error!("because: {}", cause));

    //         // The backtrace is not always generated. Try to run this example
    //         // with `$env:RUST_BACKTRACE=1`.
    //         //    if let Some(backtrace) = e.backtrace() {
    //         //        log::debug!("backtrace: {:?}", backtrace);
    //         //    }

    //         ::std::process::exit(1);
    //     }
}
