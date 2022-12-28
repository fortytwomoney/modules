use cosmwasm_std::{DepsMut, Env, Reply, StdResult, Response, StdError};

use forty_two::autocompounder::state::CONFIG;
use protobuf::Message;

use crate::contract::{AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID};

pub fn reply_handler(
    _deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    match reply.id {
        INSTANTIATE_REPLY_ID => instantiate_reply(_deps, _env, _app, reply),
    }

    
}

pub fn instantiate_reply(
    _deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;
    
    let vault_token_addr = res.get_contract_address();

    CONFIG.update(_deps.storage, |mut config| -> StdResult<_> {
        config.lp_token = vault_token_addr.parse()?;
        Ok(config)
    })?;

    Ok(Response::new().add_attribute("vault_token_addr", vault_token_addr))
}