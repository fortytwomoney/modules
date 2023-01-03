use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};

use abstract_sdk::os::objects::AnsAsset;
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, QuerierWrapper,
    QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmQuery,
};
use cw20::{AllowanceResponse, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};

use cw_asset::{AssetInfo, AssetList};
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};


use crate::contract::{AutocompounderApp, AutocompounderResult, LP_PROVISION_REPLY_ID};
use crate::error::AutocompounderError;
use crate::state::{CACHED_USER_ADDR, CONFIG};

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
        AutocompounderExecuteMsg::Deposit { funds } => deposit(deps, info, _env, app, funds),
        _ => Err(AutocompounderError::ExceededMaxCount {}),
        AutocompounderExecuteMsg::Withdraw {} => todo!(),
        AutocompounderExecuteMsg::Compound {} => todo!(),
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
pub fn deposit(
    deps: DepsMut,
    msg_info: MessageInfo,
    env: Env,
    app: AutocompounderApp,
    funds: Vec<AnsAsset>,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    let config = CONFIG.load(deps.storage)?;
    let _staking_address = config.staking_contract;
    let ans_host = app.ans_host(deps.as_ref())?;

    let messages: Vec<CosmosMsg> = vec![];

    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits.deduct_many(&msg_info.funds.clone().into())?.purge();

    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>,_> = claimed_deposits.into_iter().map(
        |asset| {
            // transfer cw20 tokens to the OS
            // will fail if allowance is not set or if some other assets are sent
            asset.transfer_from_msg(&msg_info.sender, app.proxy_address(deps.as_ref())?)
        }
    ).collect();

    // NOTE: We can still check for the allowance if you guys prefer to do it but it's functionally not required.

    // check if funds have proper amount/allowance [Check previous TODO]
    // for asset in funds.clone() {
    //     let info = asset.resolve(&deps.querier, &ans_host)?;
    //     let sent_funds = match info.clone() {
    //         AssetInfo::Native(denom) => msg_info
    //             .funds
    //             .iter()
    //             .filter(|c| c.denom == denom)
    //             .map(|c| c.amount)
    //             .sum::<Uint128>(),
    //         AssetInfo::Cw20(contract_addr) => {
    //             let allowance: AllowanceResponse = deps.querier.query_wasm_smart(
    //                 contract_addr,
    //                 &cw20::Cw20QueryMsg::Allowance {
    //                     owner: msg_info.sender.clone().into_string(),
    //                     spender: env.contract.address.clone().into_string(),
    //                 },
    //             )?;

    //             allowance.allowance
    //         }
    //         _ => {
    //             return Err(StdError::generic_err("asset type not supported".to_string()).into());
    //         }
    //     };
    //     if sent_funds != asset.amount {
    //         return Err(AutocompounderError::FundsMismatch {
    //             sent: sent_funds,
    //             wanted: asset.amount,
    //         });
    //     }
    // }

    // transfer received coins to the bank contract
    let bank = app.bank(deps.as_ref());
    bank.deposit_coins(msg_info.funds)?;

    let modules = app.modules(deps.as_ref());
    let swap_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.dex.into(),
            action: DexAction::ProvideLiquidity {
                assets: funds,
                max_spread: None,
            },
        },
    )?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: swap_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &msg_info.sender)?;
    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "4T2/AC/Deposit"))
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

fn redeem(deps: DepsMut, _env: Env, sender: String, _amount: Uint128) -> AutocompounderResult {
    let _config = CONFIG.load(deps.storage)?;

    // TODO: check that withdrawals are enabled

    // parse sender
    let _sender = deps.api.addr_validate(&sender)?;

    // TODO: calculate the size of vault and the amount of assets to withdraw

    // TODO: create message to send back underlying tokens to user

    // TODO: burn liquidity tokens

    Ok(Response::default())
}

// fn get_token_amount(
//     deps: DepsMut,
//     env: Env,
//     sender: String,
//     amount: Uint128,
// ) -> AutocompounderResult {
//     let config = CONFIG.load(deps.storage)?;
// }

fn get_token_info(querier: &QuerierWrapper, contract_addr: Addr) -> StdResult<TokenInfoResponse> {
    let token_info: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(token_info)
}
