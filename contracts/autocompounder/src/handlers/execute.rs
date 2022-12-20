use cosmwasm_std::{DepsMut, Env, MessageInfo, Uint128};
use cw_asset::Asset;
use forty_two::autocompounder::AutocompounderExecuteMsg;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;

/// Handle the `AutocompounderExecuteMsg`s sent to this app.
pub fn execute_handler(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderExecuteMsg,
) -> AutocompounderResult {
    match msg {
        AutocompounderExecuteMsg::UpdateFeeConfig {
            performance,
            withdrawal,
            deposit,
        } => update_fee_config(deps, info, app, performance, withdrawal, deposit),
        AutocompounderExecuteMsg::Zap { pool, funds } => zap(deps, info, _env, app, pool, funds),
        _ => Err(AutocompounderError::ExceededMaxCount {}),
    }
}

/// Update the application configuration.
pub fn update_fee_config(
    deps: DepsMut,
    msg_info: MessageInfo,
    dapp: AutocompounderApp,
    _fee: Option<Uint128>,
    _withdrawal: Option<Uint128>,
    _deposit: Option<Uint128>,
) -> AutocompounderResult {
    dapp.admin.assert_admin(deps.as_ref(), &msg_info.sender)?;

    unimplemented!()
}

// im assuming that this is the function that will be called when the user wants to pool AND stake their funds
pub fn zap(
    deps: DepsMut,
    msg_info: MessageInfo,
    env: Env,
    dapp: AutocompounderApp,
    pool: String,
    funds: Vec<Asset>,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    deps.api.addr_validate(&pool)?;
    // TODO: Swap the funds into 50/50. Might not be nescesarry with dex module single sided add liquidity
    
    // TODO: get the liquidity token amount

    // TODO: stake the liquidity token

    unimplemented!()
}
