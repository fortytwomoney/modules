/// This file contains the manually written protobuf encoding for the kujira token factory messages.
/// The protobuf file can be found here: https://github.com/Team-Kujira/core/blob/master/proto/denom/tx.proto
/// A mapping of the typeUrls can be found here: https://github.com/Team-Kujira/kujira.js/blob/master/src/kujira/kujira.denom/index.ts
///
///
/// NOTE on MsgCreateDenom.nonce:
/// Hans, [21 Aug 2023 at 16:21:07]: subdenom in the custom bindings maps to the nonce parameter in MsgCreateDenom https://github.com/Team-Kujira/core/blob/master/x/denom/wasm/interface_msg.go#L74
use anybuf::Anybuf;
use cosmwasm_std::Uint128;

/// Encodes a Kujira's MsgCreateDenom message to binary.
/// Denom will be in the format: factory/{sender}/{`denom`}.
/// Sources:
/// - [kujiras protobufs](https://github.com/Team-Kujira/core/blob/master/proto/denom/tx.proto).
/// - [kujiras message types](https://github.com/Team-Kujira/kujira.js/blob/master/src/kujira/kujira.denom/index.ts)
/// ```ignore
/// // MsgCreateDenom is the sdk.Msg type for allowing an account to create
/// // a new denom.  It requires a sender address and a unique nonce
/// // (to allow accounts to create multiple denoms)
/// message MsgCreateDenom {
///   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
///   string nonce = 2 [ (gogoproto.moretags) = "yaml:\"nonce\"" ]; // unique nonce. Mapped by kujira to be the CreateSubDenom(?)
/// }
/// ```
pub fn encode_msg_create_denom(sender: &str, denom: &str) -> Vec<u8> {
    // like from their docs: https://docs.kujira.app/developers/smart-contracts/token-factory#creation
    Anybuf::new()
        .append_string(1, sender)
        .append_string(2, denom)
        .into_vec()
}

/// // MsgMint is the sdk.Msg type for allowing an admin account to mint
/// more of a token.
/// ```ignore
/// message MsgMint {
///   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
///   cosmos.base.v1beta1.Coin amount = 2 [
///     (gogoproto.moretags) = "yaml:\"amount\"",
///     (gogoproto.nullable) = false
///   ];
///   string recipient = 3 [ (gogoproto.moretags) = "yaml:\"recipient\"" ];
/// }
pub fn encode_msg_mint(sender: &str, denom: &str, amount: Uint128) -> Vec<u8> {
    let coin = Anybuf::new()
        .append_string(1, denom)
        .append_string(2, amount.to_string());

    Anybuf::new()
        .append_string(1, sender)
        .append_message(2, &coin)
        .into_vec()
}

/// // MsgBurn is the sdk.Msg type for allowing an admin account to burn
/// // a token.  For now, we only support burning from the sender account.
/// ```ignore
/// message MsgBurn {
///   string sender = 1 [ (gogoproto.moretags) = "yaml:\"sender\"" ];
///   cosmos.base.v1beta1.Coin amount = 2 [
///     (gogoproto.moretags) = "yaml:\"amount\"",
///     (gogoproto.nullable) = false
///   ];
/// }
/// ```
pub fn encode_msg_burn(sender: &str, denom: &str, amount: Uint128) -> Vec<u8> {
    let coin = Anybuf::new()
        .append_string(1, denom)
        .append_string(2, amount.to_string());

    Anybuf::new()
        .append_string(1, sender)
        .append_message(2, &coin)
        .into_vec()
}

/// Encodes the stargate query message to get the total supply of a denom.
/// protobuf source: https://github.com/cosmos/cosmos-sdk/blob/c0fe4f7da17b7ec17d9bea6fcb57b4644f044b7a/proto/cosmos/bank/v1beta1/query.proto#L147-L150
/// ```ignore
/// // QuerySupplyOfRequest is the request type for the Query/SupplyOf RPC method.
/// message QuerySupplyOfRequest {
///   // denom is the coin denom to query balances for.
///   string denom = 1;
/// }
/// ```
///
pub fn encode_query_supply_of(denom: &str) -> Vec<u8> {
    Anybuf::new().append_string(1, denom).into_vec()
}
