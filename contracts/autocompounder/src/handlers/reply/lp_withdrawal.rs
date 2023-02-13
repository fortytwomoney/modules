use cosmwasm_std::{DepsMut, Env, Reply, Response};
use abstract_sdk::os::objects::AnsAsset;
use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::{Resolve, TransferInterface};
use abstract_sdk::apis::respond::AbstractResponse;
use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::state::{CACHED_USER_ADDR, CONFIG};

pub fn lp_withdrawal_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    let mut messages = vec![];
    let mut funds: Vec<AnsAsset> = vec![];
    for asset in config.pool_data.assets {
        let asset_info = asset.resolve(&deps.querier, &ans_host)?;
        let amount = asset_info.query_balance(&deps.querier, proxy_address.to_string())?;
        funds.push(AnsAsset::new(asset, amount));
    }

    let bank = app.bank(deps.as_ref());
    let transfer_msg = bank.transfer(funds, &user_address)?;
    messages.push(transfer_msg);

    let response = Response::new().add_messages(messages);
    Ok(app.tag_response(response, "lp_withdrawal_reply"))
}
