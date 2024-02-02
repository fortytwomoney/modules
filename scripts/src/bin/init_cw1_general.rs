// use cw1_general::msg::InstantiateMsg as Cw1InstantiateMsg;
// use cw_orch::daemon::{NetworkInfo, ChainInfo, ChainKind};
// use cw_orch::deploy::Deploy;
// use cw_orch::environment::{CwEnv, TxResponse};
// use cw_orch::prelude::*;
// use std::sync::Arc;

// use clap::Parser;
// use cw1_general::contract::Cw1General;
// use cw1_general::contract::CONTRACT_NAME;
// use cw_orch::prelude::{networks::parse_network, DaemonBuilder};

// fn init_cw1(args: Arguments) -> anyhow::Result<()> {
//     let network = parse_network(&args.network_id).unwrap();

//     println!("{:?}",network.grpc_urls);
//     let rt = Arc::new(tokio::runtime::Runtime::new()?);
//     let chain = DaemonBuilder::default()
//         .handle(rt.handle())
//         .chain(network)
//         .build()?;

//     let cw1 = Cw1General::new(CONTRACT_NAME, chain.clone());
//     cw1.upload()?;
//     cw1.code_id()?;
//     cw1.instantiate(&Cw1InstantiateMsg {}, None, None)?;

//     Ok(())
// }

fn main() {
    //     dotenv().ok();

    //     use dotenv::dotenv;
    //     env_logger::init();

    //     let args = Arguments::parse();

    //     if let Err(ref err) = init_cw1(args) {
    //         log::error!("{}", err);
    //         err.chain()
    //             .skip(1)
    //             .for_each(|cause| log::error!("because: {}", cause));

    //         std::process::exit(1);
    //     }
}

// #[derive(Parser, Default, Debug)]
// struct Arguments {
//     #[arg(short, long)]
//     network_id: String,
// }
