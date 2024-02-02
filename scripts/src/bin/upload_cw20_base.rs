use cw_orch::prelude::*;
use std::sync::Arc;

use cw_plus_interface::cw20_base::Cw20Base;

use clap::Parser;

use cw_orch::prelude::{networks::parse_network, DaemonBuilder};

fn upload_cw20base(args: Arguments) -> anyhow::Result<()> {
    let network = parse_network(&args.network_id).unwrap();

    println!("{:?}", network.grpc_urls);
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;

    let cw20base = Cw20Base::new("cw20_base", chain);

    cw20base.upload()?;
    println!("cw20Base codeId:{:?}", cw20base.code_id()?);

    Ok(())
}

fn main() {
    dotenv().ok();

    use dotenv::dotenv;
    env_logger::init();

    let args = Arguments::parse();

    if let Err(ref err) = upload_cw20base(args) {
        log::error!("{}", err);
        err.chain()
            .skip(1)
            .for_each(|cause| log::error!("because: {}", cause));

        std::process::exit(1);
    }
}

#[derive(Parser, Default, Debug)]
struct Arguments {
    #[arg(short, long)]
    network_id: String,
}
