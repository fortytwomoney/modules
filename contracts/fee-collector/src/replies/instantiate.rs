use crate::contract::{FeeCollectorApp, FeeCollectorResult};

use abstract_sdk::features::AbstractResponse;
use cosmwasm_std::{DepsMut, Env, Reply};

/// An example of a reply handler mapped to a reply id.
pub fn instantiate_reply(
    _deps: DepsMut,
    _env: Env,
    app: FeeCollectorApp,
    reply: Reply,
) -> FeeCollectorResult {
    let _data = reply.result.unwrap().data.unwrap();

    Ok(app.response("instantiate_reply"))
}
