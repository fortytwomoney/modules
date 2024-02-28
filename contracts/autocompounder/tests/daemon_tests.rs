use autocompounder::kujira_tx::{
    denom_params_path, encode_msg_burn, encode_msg_create_denom, encode_msg_mint,
    encode_query_supply_of, format_tokenfactory_denom, max_subdenom_length_for_chain,
    msg_burn_type_url, msg_create_denom_type_url, msg_mint_type_url, SUPPLY_OF_PATH,
};
use cosmrs::{
    rpc::{Client, HttpClient},
    tendermint::block::Height,
    Any,
};
use cw_orch::prelude::BankQuerier;
use cw_orch::prelude::NodeQuerier;
use cw_orch::prelude::QueryHandler;
use cw_orch::{
    daemon::{DaemonError, TxBuilder, Wallet},
    prelude::{
        networks::parse_network,
        queriers::{Bank, Node},
        Daemon, TxHandler,
    },
};

use speculoos::{assert_that, result::ResultAssertions};
use test_case::test_case;
use tokio::runtime::Runtime;
const LOCAL_MNEMONIC: &str = "notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius";

#[test_case("harpoon-4", "kujira"; "testing for kujira testnet")]
#[test_case("osmo-test-5", "osmosis"; "testing for osmosis testnet")]
#[serial_test::serial]
pub fn denom_query_msgs(chain_id: &str, chain_name: &str) {
    // There are two types of daemon, sync and async. Sync daemons can be used is generic code. Async daemons can be used
    // in async code (e.g. tokio), which enables multi-threaded and non-blocking code.

    // We start by creating a runtime, which is required for a sync daemon.
    let rt = Runtime::new().unwrap();

    let network = parse_network(chain_id).unwrap();
    let denom = network.gas_denom;
    // We can now create a daemon. This daemon will be used to interact with the chain.
    let daemon = Daemon::builder()
        // set the network to use
        .chain(network)
        // .chain(networks::kujira::HARPOON_4)
        .handle(rt.handle())
        .mnemonic(LOCAL_MNEMONIC)
        .build()
        .unwrap();

    // We can now use the daemon to interact with the chain. For example, we can query the total supply of a token.
    // let denom = "ukuji";
    // let chain = "kujira".to_string();

    println!("{:?}", daemon.sender());

    let bank_querier = Bank::new(&daemon);
    let supply = bank_querier.supply_of(denom).unwrap();
    // let supply = rt
    //     .block_on(bank_querier.supply_of(denom))
    //     .map_err(|e| {
    //         eprintln!("Error: {:?}", e);
    //     })
    //     .unwrap();
    println!("Daemon Bank supply: {:?}", supply);

    let data = encode_query_supply_of(denom);
    let client = HttpClient::new(
        format!("https://{chain_name}-testnet-rpc.polkachu.com")
            .to_string()
            .as_str(),
    )
    .unwrap();

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
    let supply_of_coin = supply_of_coin.replace(denom, "");
    assert_that!(supply_of_coin).is_equal_to(supply.amount.to_string());

    // query token factory params
    let response =
        rt.block_on(client.abci_query(Some(denom_params_path(chain_name)), vec![], None, true));

    println!("tokenfactory params response: {:?}", response);
    let result: String = response
        .unwrap()
        .value
        .into_iter()
        .filter_map(|val| std::char::from_u32(val as u32))
        .collect();
    println!("decoded response value: {:?}", result);
}

// this string is 140 characters long and the max for the sdk is 127
const LONG_TEST_DENOM: &str = "_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789_123456789";

#[test_case("harpoon-4", "kujira"; "testing for kujira testnet")]
#[test_case("osmo-test-5", "osmosis"; "testing for osmosis testnet")]
#[serial_test::serial]
fn tokefactory_create_mint_burn(chain_id: &str, chain_name: &str) {
    // We start by creating a runtime, which is required for a sync daemon.
    let rt = Runtime::new().unwrap();

    let network = parse_network(chain_id).unwrap();
    // We can now create a daemon. This daemon will be used to interact with the chain.
    let daemon = Daemon::builder()
        // set the network to use
        .chain(network)
        // .chain(networks::kujira::HARPOON_4)
        .handle(rt.handle())
        .mnemonic(LOCAL_MNEMONIC)
        .build()
        .unwrap();

    let block_height = daemon.block_info().unwrap().height as u32;
    let timeout_height = Height::from(block_height + 20u32);
    let wallet = daemon.wallet();

    // ------- Create Denom -----------
    // let msg = tokenfactory_create_denom_msg(daemon.sender().to_string(), "4T2TEST1".to_string()).unwrap()
    fn create_mint_burn_msgs(
        sender_str: &str,
        chain_name: &str,
        new_subdenom: &str,
        factory_denom: &str,
    ) -> Vec<Any> {
        let create_denom_msg = Any {
            type_url: msg_create_denom_type_url(chain_name),
            value: encode_msg_create_denom(sender_str, new_subdenom, chain_name),
        };
        let any_mint_msg = Any {
            type_url: msg_mint_type_url(chain_name),
            value: encode_msg_mint(sender_str, factory_denom, 1_000_000u128.into(), sender_str),
        };
        let any_burn_msg = Any {
            type_url: msg_burn_type_url(chain_name),
            value: encode_msg_burn(sender_str, factory_denom, 1_000_000u128.into()),
        };

        vec![create_denom_msg, any_mint_msg, any_burn_msg]
    }

    // let truncated_subdenom = LONG_TEST_DENOM.to_string().clone();
    // let factory_denom = format_tokenfactory_denom(daemon.sender().as_str(), &truncated_subdenom.as_str());
    // let short_denom_test_msgs = create_mint_burn_msgs(daemon.sender().as_str(), chain_name, &truncated_subdenom, &factory_denom);
    // let tx_response = rt.block_on(simulate_any_msg(
    //     &wallet,
    //     short_denom_test_msgs,
    //     timeout_height,
    // ));

    // let response = assert_that!(tx_response).is_err().subject;
    // dbg!(format!(
    //     "simulated creation, mint, and burn of too long denom {:} failed.
    //     gas response: {:?}
    //     factory_denom: {:}",
    //     truncated_subdenom, response, factory_denom
    // ));

    let mut truncated_subdenom = LONG_TEST_DENOM.to_string().clone();
    truncated_subdenom.truncate(max_subdenom_length_for_chain(chain_name));
    let factory_denom =
        format_tokenfactory_denom(daemon.sender().as_str(), truncated_subdenom.as_str());

    let short_denom_test_msgs = create_mint_burn_msgs(
        daemon.sender().as_str(),
        chain_name,
        &truncated_subdenom,
        &factory_denom,
    );

    let tx_response = simulate_any_msg(&wallet, &daemon, short_denom_test_msgs, timeout_height);

    let response = assert_that!(tx_response).is_ok().subject;
    dbg!(format!(
        "simulated creation, mint, and burn of denom {:} succesful.
        gas response: {:?}
        factory_denom: {:}",
        truncated_subdenom, response, factory_denom
    ));
}

fn simulate_any_msg(
    wallet: &Wallet,
    daemon: &Daemon,
    any_msgs: Vec<Any>,
    timeout_height: Height,
) -> Result<u64, DaemonError> {
    let tx_body = cosmrs::tx::Body::new(any_msgs, "The answer is 42", timeout_height);
    let mut tx_builder = TxBuilder::new(tx_body);

    let raw_tx = daemon.rt_handle.block_on(tx_builder.build(wallet)).unwrap();
    Node::new(daemon).simulate_tx(raw_tx.to_bytes()?)
}
