use cosmwasm_std::{DepsMut, Env, Reply, Response, StdError, StdResult, Uint128};

use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID, LP_PROVISION_REPLY_ID,
};
use crate::state::{Config, CONFIG};

use crate::response::MsgInstantiateContractResponse;

pub fn reply_handler(
    _deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    match reply.id {
        INSTANTIATE_REPLY_ID => instantiate_reply(_deps, _env, _app, reply),
        LP_PROVISION_REPLY_ID => lp_provision_reply(_deps, _env, _app, reply),
        _ => StdError::generic_err("Unknown reply id"),
    }
<<<<<<< HEAD


=======
>>>>>>> 50e42ec (check funds have proper amount/allowance + transfer assets to contract)
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

pub fn lp_provision_reply(
    _deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();

    // 1) get the amount of LP tokens minted and the amount of LP tokens already owned by the proxy

    // LP tokens minted in this transaction
    let new_lp_token_minted: Uint128; // TODO: get from reply

    // LP tokens currently owned by the proxy (includes the amount of LP tokens minted in this transaction)
    let lp_token_owned: Uint128; // TODO: get from lp contract query

    // Current amount of vault tokens in circulation
    let current_vault_amount: Uint128; // TODO: get from vault token contract query

    // amount of lp tokens staked by the proxy before this transaction
    let prev_lp_token_amount = new_lp_token_minted.checked_sub(lp_token_owned).unwrap();

    // The total value of all LP tokens that are staked by the proxy are equal to the total value of all vault tokens in circulation
    // mint_amount =  (current_vault_amount / lp_token_minted) * new_lp_tokens_minted

    let mint_amount = Uint128::zero(); // TODO: calculate

    Ok(Response::new().add_attribute("vault_token_minted", mint_amount))
}
