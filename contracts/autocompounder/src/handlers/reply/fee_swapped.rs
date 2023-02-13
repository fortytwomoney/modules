use cosmwasm_std::{DepsMut, Env, Reply, Response};
use abstract_sdk::{
    Resolve,
    TransferInterface,
    os::objects::AnsAsset,
    base::features::{AbstractNameService, Identification},
    apis::respond::AbstractResponse
};
use forty_two::autocompounder::FeeConfig;
use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::state::FEE_CONFIG;

pub fn fee_swapped_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let FeeConfig {
        fee_asset,
        commission_addr,
        ..
    } = FEE_CONFIG.load(deps.storage)?;

    let fee_balance = fee_asset
        .resolve(&deps.querier, &app.ans_host(deps.as_ref())?)?
        .query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;

    let transfer_msg = app.bank(deps.as_ref()).transfer(
        vec![AnsAsset::new(fee_asset, fee_balance)],
        &commission_addr,
    )?;

    let response = Response::new().add_message(transfer_msg);
    Ok(app.tag_response(response, "transfer_platform_fees"))
}
