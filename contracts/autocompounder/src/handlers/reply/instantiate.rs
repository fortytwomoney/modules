use cosmwasm_std::{Addr, DepsMut, Env, Reply, Response, StdError, StdResult};
use protobuf::Message;
use abstract_sdk::apis::respond::AbstractResponse;
use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::response::MsgInstantiateContractResponse;
use crate::state::CONFIG;

/// Handle a relpy for the [`INSTANTIATE_REPLY_ID`] reply.
pub fn instantiate_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    let vault_token_addr = res.get_contract_address();

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.vault_token = Addr::unchecked(vault_token_addr);
        Ok(config)
    })?;

    Ok(app.custom_tag_response(
        Response::new(),
        "instantiate",
        vec![("vault_token_addr", vault_token_addr)],
    ))
}
