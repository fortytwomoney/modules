use abstract_sdk::base::features::AbstractNameService;
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg, OfferAsset};

use abstract_sdk::{ModuleInterface, TransferInterface};
use abstract_sdk::register::EXCHANGE;
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, QuerierWrapper,
    QueryRequest, Response, StdError, StdResult, SubMsg, Uint128, WasmQuery, ReplyOn,
};
use cw20::{AllowanceResponse, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};
use cw_asset::{Asset, AssetInfo};
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};

use crate::contract::{AutocompounderApp, AutocompounderResult, LP_PROVISION_REPLY_ID};
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
    funds: Vec<OfferAsset>,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    let config = CONFIG.load(deps.storage)?;

    let dex_pair = app.name_service(deps.as_ref()).query(&config.dex_pair)?;
    let staking_address = Addr::unchecked("");
    let staking_proxy_balance: Uint128 = Uint128::zero(); // TODO
    let value_of_staking_proxy_balance: Decimal = Decimal::zero(); // TODO

    let bank = app.bank(deps.as_ref());
    // TODO: ask Howard
    bank.deposit(funds)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // check if funds have proper amount/allowance
    for asset in funds {
        let sent_funds = match asset.info.clone() {
            AssetInfo::Native(denom) => msg_info
                .funds
                .iter()
                .filter(|c| c.denom == denom)
                .map(|c| c.amount)
                .sum::<Uint128>(),
            AssetInfo::Cw20(contract_addr) => {
                let allowance: AllowanceResponse = deps.querier.query_wasm_smart(
                    contract_addr,
                    &cw20::Cw20QueryMsg::Allowance {
                        owner: msg_info.sender.clone().into_string(),
                        spender: env.contract.address.clone().into_string(),
                    },
                )?;

                allowance.allowance
            }
            _ => {
                return Err(StdError::generic_err("asset type not supported".to_string()).into());
            }
        };
        if sent_funds != asset.amount {
            return Err(AutocompounderError::FundsMismatch {
                sent: sent_funds,
                wanted: asset.amount,
            });
        }
        // add cw20 transfer message if needed
        if let AssetInfo::Cw20(contract_addr) = asset.info.clone() {
            messages.push(
                asset.transfer_from_msg(msg_info.sender.clone(), env.contract.address.clone())?,
            )
        }
    }

    // get total vault shares
    let total_vault_shares: TokenInfoResponse =
        get_token_info(&deps.querier, config.liquidity_token.clone())?.total_supply;

    // // calculate vault tokens to mint
    // let vault_tokens_amount_to_mint = if total_vault_shares.is_zero() {
    //     // first depositor to the vault, mint LP tokens 1:1
    //     amount
    // }
    let modules = app.modules(deps.as_ref());
    let swap_msg: CosmosMsg = modules.api_request(EXCHANGE, DexExecuteMsg {
        dex: config.dex.into(),
        action: DexAction::ProvideLiquidity {
            assets: funds,
            max_spread: None,
        },
    })?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: swap_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    Ok(
        Response::new()
            .add_submessage(sub_msg)
            .add_attribute("action", "4T2/AC/Deposit")
    )
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

fn get_token_amount(
    deps: DepsMut,
    env: Env,
    sender: String,
    amount: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
}

fn get_token_info(querier: &QuerierWrapper, contract_addr: Addr) -> AutocompounderResult {
    let token_info: TokenInfoResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: contract_addr.to_string(),
        msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(token_info)
}
