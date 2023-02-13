use cosmwasm_std::{CosmosMsg, Decimal, DepsMut, Env, Reply, Response, StdResult, SubMsg};
use abstract_sdk::os::objects::AnsAsset;
use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::apis::dex::DexInterface;
use abstract_sdk::Resolve;
use abstract_sdk::apis::respond::AbstractResponse;
use crate::contract::{AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID};
use crate::state::CONFIG;

/// Queries the balances of pool assets and provides liquidity to the pool
///
/// This function is triggered after the last swap message of the lp_compound_reply
/// and assumes the contract has no other rewards than the ones in the pool assets
pub fn swapped_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let ans_host = app.ans_host(deps.as_ref())?;
    let config = CONFIG.load(deps.storage)?;
    let dex = app.dex(deps.as_ref(), config.pool_data.dex);

    // 1) query balance of pool tokens
    let rewards = config
        .pool_data
        .assets
        .iter()
        .map(|entry| -> StdResult<AnsAsset> {
            let tkn = entry.resolve(&deps.querier, &ans_host)?;
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps.as_ref())?)?;
            Ok(AnsAsset::new(entry.clone(), balance))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;

    // 2) provide liquidity
    let lp_msg: CosmosMsg = dex.provide_liquidity(rewards, Some(Decimal::percent(10)))?;
    let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

    let response = Response::new().add_submessage(submsg);
    Ok(app.tag_response(response, "provide_liquidity"))
}
