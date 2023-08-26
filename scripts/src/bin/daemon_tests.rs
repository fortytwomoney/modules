use autocompounder::kujira_tx::{encode_query_supply_of, tokenfactory_create_denom_msg, encode_msg_create_denom, encode_msg_burn, encode_msg_mint, format_tokenfactory_denom};
use cw_orch::{prelude::{
    networks,
    Daemon, TxHandler, queriers::{DaemonQuerier, Bank, Node},
}, daemon::{TxBuilder, Wallet, CosmTxResponse, DaemonError}};
use cosmrs::{rpc::{endpoint::{abci_query, broadcast::tx_async::Response}, Client, HttpClient}, tendermint::{serializers::bytes::base64string, block::Height}, tx::{Msg, Raw, Body}, Any, proto::cosmos::tx::v1beta1::TxBody};

use tokio::runtime::Runtime;
const LOCAL_MNEMONIC: &str = "notice oak worry limit wrap speak medal online prefer cluster roof addict wrist behave treat actual wasp year salad speed social layer crew genius";


pub fn main() {
    // There are two types of daemon, sync and async. Sync daemons can be used is generic code. Async daemons can be used
    // in async code (e.g. tokio), which enables multi-threaded and non-blocking code.

    env_logger::init();
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
    let timeout_height = Height::from(block_height  + 20u32);
    let wallet = daemon.wallet();

    // We can now use the daemon to interact with the chain. For example, we can query the total supply of a token.
    let denom = "ukuji";

    println!("{:?}",  daemon.sender());

    let bank_querier = Bank::new(daemon.channel());
    let supply = rt.block_on(bank_querier.supply_of(denom)).unwrap();
    println!("Daemon Bank supply: {:?}", supply);

    let data = encode_query_supply_of(denom);
    let client = HttpClient::new("https://rpc.cosmos.directory/kujira").unwrap();

    // Querying
    let response = rt.block_on(client.abci_query(
            Some("/cosmos.bank.v1beta1.Query/SupplyOf".to_string()),
            data,
            None,
            true,
        )).unwrap()
    ;



    // // convert response value like: `[10, 10, 10, 5, 117, 107, 117, 106, 105, 18, 1, 48]`
    println!("response: {:?}", response);
    let result: String = response.value.into_iter().filter_map(|val| std::char::from_u32(val as u32)).collect();
    println!("decoded response value: {:?}", result);

    // query token factory params 
    let response = rt.block_on(
        client.abci_query(
            Some("/kujira.denom.Query/Params".to_string()),
            vec![],
            None,
            true,
        )
    );

    println!("tokenfactory params response: {:?}", response);
    let result: String = response.unwrap().value.into_iter().filter_map(|val| std::char::from_u32(val as u32)).collect();
    println!("decoded response value: {:?}", result);

    // ------- Create Denom -----------
    let new_subdenom = "4T2TEST2";
    let factory_denom = format_tokenfactory_denom(daemon.sender().as_str(), new_subdenom);

    // let msg = tokenfactory_create_denom_msg(daemon.sender().to_string(), "4T2TEST1".to_string()).unwrap()
    let create_denom_msg = Any {
        type_url: "/kujira.denom.MsgCreateDenom".to_string(),
        value: encode_msg_create_denom(daemon.sender().as_str(), new_subdenom)
    };
    let any_mint_msg = Any {
        type_url: "/kujira.denom.MsgMint".to_string(),
        value: encode_msg_mint(daemon.sender().as_str(), &factory_denom, 1_000_000u128.into(), daemon.sender().as_str())
    };
    let any_burn_msg = Any {
        type_url: "/kujira.denom.MsgBurn".to_string(),
        value: encode_msg_burn(daemon.sender().as_str(), &factory_denom, 1_000_000u128.into())
    };
    
    let tx_response = rt.block_on(
        simulate_any_msg(&wallet, vec![
            create_denom_msg.clone(),
            any_mint_msg.clone(),
            any_burn_msg.clone()
        ], timeout_height)
    );
    
    let Ok(response) = tx_response else {
        println!("Error: {:?}", tx_response); return;
    };

    println!(
        "simulated creation, mint, and burn of denom {:} succesful.
        gas response: {:?}
        factory_denom: {:}", 
        new_subdenom, 
        response, 
        factory_denom);
    
    }


async fn simulate_any_msg(wallet: &Wallet, any_msgs: Vec<Any>, timeout_height:Height) -> Result<u64, DaemonError> {
        let tx_body = cosmrs::tx::Body::new(any_msgs, "The answer is 42", timeout_height);
    let mut tx_builder = TxBuilder::new(tx_body);

    let raw_tx = tx_builder.build(wallet).await.unwrap();
    Node::new(wallet.channel()).simulate_tx(raw_tx.to_bytes()?).await
}

async fn broadcast_with_gas(daemon: &Daemon, any_msg: Any, timeout_height:Height) -> Result<CosmTxResponse, DaemonError> {
    let wallet = daemon.wallet();
    let tx_body = cosmrs::tx::Body::new(vec![any_msg], "The answer is 42", timeout_height);

    let mut tx_builder = TxBuilder::new(tx_body);
    let tx = tx_builder.build(wallet.as_ref()).await?;

    let mut tx_response = wallet.broadcast_tx(tx).await?;

        log::debug!("tx broadcast response: {:?}", tx_response);

        if has_insufficient_fee(&tx_response.raw_log) {
            // get the suggested fee from the error message
            let suggested_fee = parse_suggested_fee(&tx_response.raw_log);

            let Some(new_fee) = suggested_fee else {
                return Err(DaemonError::InsufficientFee(
                    tx_response.raw_log,
                ));
            };

            // update the fee and try again
            tx_builder.fee_amount(new_fee);
            let tx = tx_builder.build(&wallet).await?;

            tx_response = wallet.broadcast_tx(tx).await?;
        }

        let resp = Node::new(wallet.channel())
            .find_tx(tx_response.txhash)
            .await?;

        println!("broadcasted tx: {:?} \n", resp);
        Ok(resp)
}

fn has_insufficient_fee(raw_log: &str) -> bool {
    raw_log.contains("insufficient fees")
}
fn parse_suggested_fee(raw_log: &str) -> Option<u128> {
    // Step 1: Split the log message into "got" and "required" parts.
    let parts: Vec<&str> = raw_log.split("required: ").collect();

    // Make sure the log message is in the expected format.
    // Step 2: Split the "got" part to extract the paid fee and denomination.
    let got_parts: Vec<&str> = parts[0].split_whitespace().collect();

    // Extract the paid fee and denomination.
    let paid_fee_with_denom = got_parts.last()?;
    let (_, denomination) =
        paid_fee_with_denom.split_at(paid_fee_with_denom.find(|c: char| !c.is_numeric())?);

    eprintln!("denom: {}", denomination);

    // Step 3: Iterate over each fee in the "required" part.
    let required_fees: Vec<&str> = parts[1].split(denomination).collect();

    eprintln!("required fees: {:?}", required_fees);

    // read until the first non-numeric character backwards on the first string
    let (_, suggested_fee) =
        required_fees[0].split_at(required_fees[0].rfind(|c: char| !c.is_numeric())?);
    eprintln!("suggested fee: {}", suggested_fee);

    // remove the first character if parsing errors, which can be a comma
    suggested_fee
        .parse::<u128>()
        .ok()
        .or(suggested_fee[1..].parse::<u128>().ok())
}