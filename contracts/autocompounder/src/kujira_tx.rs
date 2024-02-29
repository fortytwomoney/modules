/// This file contains the manually written protobuf encoding for the kujira token factory messages.
/// The protobuf file can be found here: https://github.com/Team-Kujira/core/blob/master/proto/denom/tx.proto
/// A mapping of the typeUrls can be found here: https://github.com/Team-Kujira/kujira.js/blob/master/src/kujira/kujira.denom/index.ts
///
///
/// NOTE on MsgCreateDenom.nonce:
/// Hans, [21 Aug 2023 at 16:21:07]: subdenom in the custom bindings maps to the nonce parameter in MsgCreateDenom https://github.com/Team-Kujira/core/blob/master/x/denom/wasm/interface_msg.go#L74
use anybuf::Anybuf;
use cosmwasm_std::{
    from_json, to_json_binary, Addr, Binary, Coin, CosmosMsg, Deps, Empty, QueryRequest, StdError,
    Uint128,
};
use serde::{Deserialize, Serialize};

pub const SUPPLY_OF_PATH: &str = "/cosmos.bank.v1beta1.Query/SupplyOf";

/// create as functions with kujira replaced as variable
pub fn msg_create_denom_type_url(chain: &str) -> String {
    tokenfactory_prefix_for(chain) + "MsgCreateDenom"
}

pub fn msg_mint_type_url(chain: &str) -> String {
    tokenfactory_prefix_for(chain) + "MsgMint"
}

pub fn msg_burn_type_url(chain: &str) -> String {
    tokenfactory_prefix_for(chain) + "MsgBurn"
}

pub fn tokenfactory_prefix_for(chain: &str) -> String {
    match chain {
        "kujira" => "/kujira.denom.".to_string(),
        "osmosis" => "/osmosis.tokenfactory.v1beta1.".to_string(),
        "wyndex" => "/juno.tokenfactory.v1beta1.".to_string(),
        _ => panic!("chain {:} not supported", chain),
    }
}

pub fn denom_params_path(chain: &str) -> String {
    tokenfactory_prefix_for(chain) + "Query/Params"
}

pub const TOKEN_FACTORY_CREATION_FEE: u128 = 100_000_000u128;

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
pub fn encode_msg_create_denom(sender: &str, denom: &str, _chain: &str) -> Vec<u8> {
    Anybuf::new()
        .append_string(1, sender)
        .append_string(2, denom)
        .into_vec()
    // like from their docs: https://docs.kujira.app/developers/smart-contracts/token-factory#creation
}

pub fn tokenfactory_create_denom_msg(minter: String, subdenom: String, chain: &str) -> CosmosMsg {
    let msg = encode_msg_create_denom(&minter, &subdenom, chain);
    CosmosMsg::Stargate {
        type_url: msg_create_denom_type_url(chain),
        value: msg.into(),
    }
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
pub fn encode_msg_mint(sender: &str, denom: &str, amount: Uint128, recipient: &str) -> Vec<u8> {
    let coin = Anybuf::new()
        .append_string(1, denom)
        .append_string(2, amount.to_string());

    Anybuf::new()
        .append_string(1, sender)
        .append_message(2, &coin)
        .append_string(3, recipient)
        .into_vec()
}

pub fn tokenfactory_mint_msg(
    minter: &Addr,
    denom: String,
    amount: Uint128,
    recipient: &str,
    chain: &str,
) -> Result<CosmosMsg, StdError> {
    let proto_msg = encode_msg_mint(minter.as_str(), denom.as_str(), amount, recipient);
    let msg = CosmosMsg::Stargate {
        type_url: msg_mint_type_url(chain),
        value: Binary(proto_msg),
    };
    Ok(msg)
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

pub fn tokenfactory_burn_msg(
    minter: &Addr,
    denom: String,
    amount: Uint128,
    chain: &str,
) -> Result<CosmosMsg, StdError> {
    let proto_msg = encode_msg_burn(minter.as_str(), &denom, amount);
    let msg = CosmosMsg::Stargate {
        type_url: msg_burn_type_url(chain),
        value: Binary(proto_msg),
    };
    Ok(msg)
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

/// Encodes the stargate query message to get the total supply of a denom.
///

pub fn encode_query_params() -> Vec<u8> {
    Anybuf::new().into_vec()
}

/// ParamsResponse is the response type for the Query/Params RPC method. https://github.com/Team-Kujira/core/blob/master/proto/denom/params.proto
/// ```ignore
/// // Params holds parameters for the denom module
/// message Params {
///   repeated cosmos.base.v1beta1.Coin creation_fee = 1 [
///     (gogoproto.castrepeated) = "github.com/cosmos/cosmos-sdk/types.Coins",
///     (gogoproto.moretags) = "yaml:\"creation_fee\"",
///     (gogoproto.nullable) = false
///   ];
/// }
/// ```
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Params {
    pub creation_fee: Coin,
}

///
///
pub fn tokenfactory_params_query_request(chain: &str) -> Result<QueryRequest<Empty>, StdError> {
    let proto_msg = encode_query_params();
    let q_request: QueryRequest<Empty> = QueryRequest::Stargate {
        path: denom_params_path(chain),
        data: to_json_binary(&proto_msg)?,
    };

    Ok(q_request)
}

pub fn query_tokenfactory_params(deps: Deps, chain: &str) -> Result<Params, StdError> {
    let q_request = tokenfactory_params_query_request(chain)?;
    let response: Binary = deps.querier.query(&q_request)?;
    let params_response: Params = from_json(response)?;
    Ok(params_response)
}

/// Formats the native denom to the asset info for the vault token with denom "factory/{`sender`}/{`denom`}"
/// As per osmosis implementation here: https://github.com/osmosis-labs/osmosis/blob/6a53f5611ae27b653a5758333c9a0862835917f4/x/tokenfactory/types/denoms.go#L34
/// and same for kujira https://github.com/Team-Kujira/core/blob/554950147825e94fa52c3ff0a3b138568cf7c774/x/denom/types/denoms.go#L23C63-L23C63
pub fn format_tokenfactory_denom(sender: &str, denom: &str) -> String {
    format!("factory/{sender}/{denom}")
}

pub const MAX_SUBDENOM_LEN_OSMOSIS: usize = 44;
pub const MAX_SUBDENOM_LEN_KUJIRA: usize = 64;

/// max length of subdenom for osmosis is 44 https://github.com/osmosis-labs/osmosis/blob/6a53f5611ae27b653a5758333c9a0862835917f4/x/tokenfactory/types/denoms.go#L10-L36
pub fn max_subdenom_length_for_chain(dex: &str) -> usize {
    match dex {
        "osmosis" => MAX_SUBDENOM_LEN_OSMOSIS,
        "kujira" => MAX_SUBDENOM_LEN_KUJIRA,
        _ => 64,
    }
}
