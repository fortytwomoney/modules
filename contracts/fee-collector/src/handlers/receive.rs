use crate::{
    contract::{FeeCollectorApp, FeeCollectorResult},
    msg::Cw20HookMsg,
};
use abstract_sdk::AbstractResponse;
use cosmwasm_std::{from_json, DepsMut, Env, MessageInfo};
use cw20::Cw20ReceiveMsg;

/// handler function invoked when the vault dapp contract receives
/// a transaction. In this case it is triggered when either a LP tokens received
/// by the contract or when the deposit asset is a cw20 asset.
pub fn receive_handler(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    app: FeeCollectorApp,
    cw20_msg: Cw20ReceiveMsg,
) -> FeeCollectorResult {
    match from_json(cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => {
            // Do nothing, just return
            Ok(app.custom_response("receive_cw20", vec![("method", "deposit")]))
        }
    }
}
