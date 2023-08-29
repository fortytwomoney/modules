use autocompounder::kujira_tx::{
    encode_msg_burn, encode_msg_create_denom, encode_msg_mint, encode_query_supply_of,
    format_tokenfactory_denom, DENOM_PARAMS_PATH, MSG_BURN_TYPE_URL, MSG_CREATE_DENOM_TYPE_URL,
    MSG_MINT_TYPE_URL, SUPPLY_OF_PATH,
};
use cosmrs::{
    rpc::{Client, HttpClient},
    tendermint::block::Height,
    Any,
};
use cw_orch::{
    daemon::{DaemonError, TxBuilder, Wallet},
    prelude::{
        networks,
        queriers::{Bank, DaemonQuerier, Node},
        Daemon, TxHandler,
    },
};

use speculoos::{assert_that, result::ResultAssertions};
use tokio::runtime::Runtime;
const LOCAL_MNEMONIC: &str = "notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius";

#[test]
pub fn denom_query_msgs() {
    // There are two types of daemon, sync and async. Sync daemons can be used is generic code. Async daemons can be used
    // in async code (e.g. tokio), which enables multi-threaded and non-blocking code.

    // We start by creating a runtime, which is required for a sync daemon.
    let rt = Runtime::new().unwrap();

    // We can now create a daemon. This daemon will be used to interact with the chain.
    let daemon = Daemon::builder()
        // set the network to use
        .chain(networks::kujira::HARPOON_4)
        .handle(rt.handle())
        .mnemonic(LOCAL_MNEMONIC)
        .build()
        .unwrap();

    // We can now use the daemon to interact with the chain. For example, we can query the total supply of a token.
    let denom = "ukuji";

    println!("{:?}", daemon.sender());

    let bank_querier = Bank::new(daemon.channel());
    let supply = rt.block_on(bank_querier.supply_of(denom)).unwrap();
    println!("Daemon Bank supply: {:?}", supply);

    let data = encode_query_supply_of(denom);
    let client = HttpClient::new("https://kujira-testnet-rpc.polkachu.com").unwrap();

    // Querying
    let response =
        rt.block_on(client.abci_query(Some(SUPPLY_OF_PATH.to_string()), data, None, true));

    let response = assert_that!(response).is_ok().subject.clone();

    // // convert response value like: `[10, 10, 10, 5, 117, 107, 117, 106, 105, 18, 1, 48]`
    let supply_of_coin: String = response
        .value
        .into_iter()
        .filter_map(|val| {
            let ch = std::char::from_u32(val as u32)?;
            if ch.is_control() {
                None
            } else {
                Some(ch)
            }
        })
        .collect();

    // remove the denom from the supply_of_coin string
    let supply_of_coin = supply_of_coin.replace("ukuji", "");
    assert_that!(supply_of_coin).is_equal_to(supply.amount.to_string());

    // query token factory params
    let response =
        rt.block_on(client.abci_query(Some(DENOM_PARAMS_PATH.to_string()), vec![], None, true));

    println!("tokenfactory params response: {:?}", response);
    let result: String = response
        .unwrap()
        .value
        .into_iter()
        .filter_map(|val| std::char::from_u32(val as u32))
        .collect();
    println!("decoded response value: {:?}", result);
}

#[test]
fn tokenfactory_create_mint_burn() {
    // We start by creating a runtime, which is required for a sync daemon.
    let rt = Runtime::new().unwrap();

    // We can now create a daemon. This daemon will be used to interact with the chain.
    let daemon = Daemon::builder()
        // set the network to use
        .chain(networks::kujira::HARPOON_4)
        .handle(rt.handle())
        .mnemonic(LOCAL_MNEMONIC)
        .build()
        .unwrap();

    let block_height = daemon.block_info().unwrap().height as u32;
    let timeout_height = Height::from(block_height + 20u32);
    let wallet = daemon.wallet();

    // ------- Create Denom -----------
    let new_subdenom = "4T2TEST2";
    let factory_denom = format_tokenfactory_denom(daemon.sender().as_str(), new_subdenom);

    // let msg = tokenfactory_create_denom_msg(daemon.sender().to_string(), "4T2TEST1".to_string()).unwrap()
    let create_denom_msg = Any {
        type_url: MSG_CREATE_DENOM_TYPE_URL.to_string(),
        value: encode_msg_create_denom(daemon.sender().as_str(), new_subdenom),
    };
    let any_mint_msg = Any {
        type_url: MSG_MINT_TYPE_URL.to_string(),
        value: encode_msg_mint(
            daemon.sender().as_str(),
            &factory_denom,
            1_000_000u128.into(),
            daemon.sender().as_str(),
        ),
    };
    let any_burn_msg = Any {
        type_url: MSG_BURN_TYPE_URL.to_string(),
        value: encode_msg_burn(
            daemon.sender().as_str(),
            &factory_denom,
            1_000_000u128.into(),
        ),
    };

    let tx_response = rt.block_on(simulate_any_msg(
        &wallet,
        vec![
            create_denom_msg.clone(),
            any_mint_msg.clone(),
            any_burn_msg.clone(),
        ],
        timeout_height,
    ));

    let response = assert_that!(tx_response).is_ok().subject;
    dbg!(
        "simulated creation, mint, and burn of denom {:} succesful.
        gas response: {:?}
        factory_denom: {:}",
        new_subdenom,
        response,
        factory_denom
    );
}

async fn simulate_any_msg(
    wallet: &Wallet,
    any_msgs: Vec<Any>,
    timeout_height: Height,
) -> Result<u64, DaemonError> {
    let tx_body = cosmrs::tx::Body::new(any_msgs, "The answer is 42", timeout_height);
    let mut tx_builder = TxBuilder::new(tx_body);

    let raw_tx = tx_builder.build(wallet).await.unwrap();
    Node::new(wallet.channel())
        .simulate_tx(raw_tx.to_bytes()?)
        .await
}
