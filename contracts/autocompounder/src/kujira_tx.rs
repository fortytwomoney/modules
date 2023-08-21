/// This file contains the manually written protobuf encoding for the kujira token factory messages.
/// The protobuf file can be found here: https://github.com/Team-Kujira/core/blob/master/proto/denom/tx.proto
/// A mapping of the typeUrls can be found here: https://github.com/Team-Kujira/kujira.js/blob/master/src/kujira/kujira.denom/index.ts 
/// 
/// 
/// NOTE on MsgCreateDenom.nonce:
/// Hans, [21 Aug 2023 at 16:21:07]: subdenom in the custom bindings maps to the nonce parameter in MsgCreateDenom https://github.com/Team-Kujira/core/blob/master/x/denom/wasm/interface_msg.go#L74
use anybuf::Anybuf;
use cosmwasm_std::Uint128;

// // MsgCreateDenom is the sdk.Msg type for allowing an account to create
// // a new denom.  It requires a sender address and a unique nonce
// // (to allow accounts to create multiple denoms)
// message MsgCreateDenom {
//   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
//   string nonce = 2 [ (gogoproto.moretags) = "yaml:\"nonce\"" ]; // unique nonce. Mapped by kujira to be the CreateSubDenom(?)
// }
pub fn encode_msg_create_denom(sender: &str, sub_denom: &str) -> Vec<u8> {
    // #TODO: Construct the subdenom based on params
    // like from their docs: https://docs.kujira.app/developers/smart-contracts/token-factory#creation
    //  let addr = env.contract.address;
    // let denom = format!("factory/{addr}/{denom}");


    let msg = Anybuf::new()
        .append_string(1, sender.to_string())
        .append_string(2, sub_denom.to_string())
        .into_vec();

    msg
}

// // MsgMint is the sdk.Msg type for allowing an admin account to mint
// more of a token. 
// message MsgMint {
//   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
//   cosmos.base.v1beta1.Coin amount = 2 [
//     (gogoproto.moretags) = "yaml:\"amount\"",
//     (gogoproto.nullable) = false
//   ];
//   string recipient = 3 [ (gogoproto.moretags) = "yaml:\"recipient\"" ];
// }
pub fn encode_msg_mint(sender: &str, denom: &str, amount: Uint128) -> Vec<u8> {
    
    let coin = Anybuf::new()
        .append_string(1, denom.to_string())
        .append_string(2, amount.to_string());

    Anybuf::new()
        .append_string(1, sender)
        .append_message(2, &coin)
        .into_vec()
}


// // MsgBurn is the sdk.Msg type for allowing an admin account to burn
// // a token.  For now, we only support burning from the sender account.
// message MsgBurn {
//   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
//   cosmos.base.v1beta1.Coin amount = 2 [
//     (gogoproto.moretags) = "yaml:\"amount\"",
//     (gogoproto.nullable) = false
//   ];
// }

pub fn encode_msg_burn(sender: &str, denom: &str, amount: Uint128) -> Vec<u8> {
    
    let coin = Anybuf::new()
        .append_string(1, denom.to_string())
        .append_string(2, amount.to_string());

    Anybuf::new()
        .append_string(1, sender)
        .append_message(2, &coin)
        .into_vec()
}
