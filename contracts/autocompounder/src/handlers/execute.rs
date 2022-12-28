use abstract_sdk::Resolve;
use abstract_sdk::base::features::AbstractNameService;
use abstract_sdk::os::objects::ans_host;
use cosmwasm_std::{from_binary, DepsMut, Env, MessageInfo, Response, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_asset::Asset;
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::state::CONFIG;

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
        AutocompounderExecuteMsg::Receive(msg) => receive(deps, info, _env, msg),
        AutocompounderExecuteMsg::Zap {  funds } => zap(deps, info, _env, app, funds),
        _ => Err(AutocompounderError::ExceededMaxCount {}),
        AutocompounderExecuteMsg::Deposit { funds } => todo!(),
        AutocompounderExecuteMsg::Withdraw {  } => todo!(),
        AutocompounderExecuteMsg::Compound {  } => todo!(),
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
    funds: Asset,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    let config = CONFIG.load(deps.storage)?;
    let dex_pair = dapp.name_service(deps.as_ref()).query( &config.dex_pair)?;


    // TODO: Swap the funds into 50/50. Might not be nescesarry with dex module single sided add liquidity

    // TODO: get the liquidity token amount

    // TODO: stake the liquidity token

    unimplemented!()
}

/// Handles receiving CW20 messages
pub fn receive(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    // Withdraw fn can only be called by liquidity token
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.liquidity_token {
        return Err(AutocompounderError::SenderIsNotLiquidityToken {});
    }

    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, msg.sender, msg.amount),
    }
}

fn redeem(deps: DepsMut, env: Env, sender: String, amount: Uint128) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // TODO: check that withdrawals are enabled
    

    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // TODO: calculate the size of vault and the amount of assets to withdraw
    
    // TODO: create message to send back underlying tokens to user

    // TODO: burn liquidity tokens

    Ok(Response::default())
}
