use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    from_binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, ReplyOn, Response, SubMsg, Uint128,
};
use cw20::Cw20ReceiveMsg;

use cw_asset::AssetList;
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};
use forty_two::cw_staking::{CwStakingAction, CwStakingExecuteMsg, CW_STAKING};

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_COMPOUND_REPLY_ID, LP_PROVISION_REPLY_ID,
};
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
        AutocompounderExecuteMsg::Withdraw {} => todo!(),
        AutocompounderExecuteMsg::Compound {} => compound(deps, info, _env, app),
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
pub fn deposit(
    deps: DepsMut,
    msg_info: MessageInfo,
    _env: Env,
    app: AutocompounderApp,
    funds: Vec<AnsAsset>,
) -> AutocompounderResult {
    // TODO: Check if the pool is valid
    let config = CONFIG.load(deps.storage)?;
    let _staking_address = config.staking_contract;
    let ans_host = app.ans_host(deps.as_ref())?;

    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits
        .deduct_many(&msg_info.funds.clone().into())?
        .purge();

    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>, _> = claimed_deposits
        .into_iter()
        .map(|asset| {
            // transfer cw20 tokens to the OS
            // will fail if allowance is not set or if some other assets are sent
            asset.transfer_from_msg(&msg_info.sender, app.proxy_address(deps.as_ref())?)
        })
        .collect();

    // transfer received coins to the bank contract
    let bank = app.bank(deps.as_ref());
    bank.deposit_coins(msg_info.funds)?;

    let modules = app.modules(deps.as_ref());
    let swap_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.dex,
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
        .add_messages(cw_20_transfer_msgs_res?)
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

fn compound(
    deps: DepsMut,
    _msg_info: MessageInfo,
    _env: Env,
    app: AutocompounderApp,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // 1) Claim rewards from staking contract
    let claim_msg = claim_lp_rewards(
        deps.as_ref(),
        &app,
        app.proxy_address(deps.as_ref())?.into_string(),
        AssetEntry::from(LpToken::from(config.pool_data)),
    );
    let claim_submsg = SubMsg {
        id: LP_COMPOUND_REPLY_ID,
        msg: claim_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // [These steps are caried out by the reply 👇]
    // 2) deduct fee from rewards and swap to native token (send to treasury?)

    // 3) Swap rewards to token in pool

    // 4) Provide liquidity to pool

    Ok(Response::new()
        .add_submessage(claim_submsg)
        .add_attribute("action", "4T2🚀AC🚀Compound🤖"))
}

fn claim_lp_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let modules = app.modules(deps);

    modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::ClaimRewards {
                    staking_token: lp_token_name,
                },
            },
        )
        .unwrap()
}
