use cosmwasm_std::{DepsMut, Env, Reply, Response};
use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::Resolve;
use abstract_sdk::apis::respond::AbstractResponse;
use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::handlers::reply;
use crate::state::CONFIG;

pub fn compound_lp_provision_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;
    let proxy = app.proxy_address(deps.as_ref())?;

    let lp_token = AssetEntry::from(LpToken::from(config.pool_data.clone()));

    // 1) query balance of lp tokens
    let lp_balance = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy)?;

    // 2) stake lp tokens
    let stake_msg = reply::stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        AnsAsset::new(lp_token, lp_balance),
        config.unbonding_period,
    )?;

    let response = Response::new().add_message(stake_msg);

    Ok(app.tag_response(response, "stake"))
}
