use cosmwasm_std::coin;
use cw_orch::prelude::{networks, Daemon};
use tokio::runtime::Runtime;

const LOCAL_MNEMONIC: &str = "notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius";

fn main() -> anyhow::Result<()> {
    let rt = Runtime::new()?;
    let daemon = Daemon::builder()
        // set the network to use
        .chain(networks::kujira::HARPOON_4)
        .handle(rt.handle())
        .mnemonic(LOCAL_MNEMONIC)
        .build()
        .unwrap();

    let ac_address = "kujira1544uzu0gjwthpwd7rsv6wtrm3j9s20ag74qxcnlp7yg5sekttnqqv00jkh";
    let _funds = "100000000ukuji";
    let wallet = daemon.wallet();

    let _res = rt.block_on(wallet.bank_send(ac_address, vec![coin(100000000, "ukuji")]))?;

    Ok(())
}
