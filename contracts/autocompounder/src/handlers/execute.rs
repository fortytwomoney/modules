use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QuerierWrapper, QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128,
    WasmQuery,
};
use cw20::{AllowanceResponse, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};

use cw_asset::AssetInfo;
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};
use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_PROVISION_REPLY_ID, LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::state::{CACHED_AMOUNT_OF_VAULT_TOKENS_TO_BURN, CACHED_USER_ADDR, CONFIG};

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

    let _bank = app.bank(deps.as_ref());

    let _messages: Vec<CosmosMsg> = vec![];

    // check if funds have proper amount/allowance [Check previous TODO]
    for asset in funds.clone() {
        let info = asset.resolve(&deps.querier, &ans_host)?.info;

        let sent_funds = match info.clone() {
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
    }

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
    env: Env,
    info: MessageInfo,
    dapp: AutocompounderApp,
    msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    // Withdraw fn can only be called by liquidity token
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.vault_token {
        return Err(AutocompounderError::SenderIsNotVaultToken {});
    }

    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, dapp, msg.sender, msg.amount),
    }
}

fn redeem(
    deps: DepsMut,
    _env: Env,
    dapp: AutocompounderApp,
    sender: String,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &sender)?;

    // save the user address to the cache for later use in reply
    CACHED_AMOUNT_OF_VAULT_TOKENS_TO_BURN
        .save(deps.storage, &amount_of_vault_tokens_to_be_burned)?;

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    let vault_tokens_total_supply = vault_token_info.total_supply;

    // 2) get total amount of LP tokens staked in vault
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let total_lp_tokens_staked_in_vault = query_stake(deps.as_ref(), &dapp, lp_token.clone());

    // 3) calculate lp tokens amount to withdraw
    let lp_tokens_withdraw_amount = Decimal::from_ratio(
        amount_of_vault_tokens_to_be_burned,
        vault_tokens_total_supply,
    ) * total_lp_tokens_staked_in_vault;

    // 4) claim lp tokens
    let claim_unbonded_lps_msg = claim_lps(
        deps.as_ref(),
        &dapp,
        "junoswap".to_string(),
        lp_token.clone(),
    );

    let modules = dapp.modules(deps.as_ref());

    let withdraw_liquidity_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.dex.into(),
            action: DexAction::WithdrawLiquidity {
                lp_token: lp_token.clone(),
                amount: lp_tokens_withdraw_amount,
            },
        },
    )?;

    let withdraw_liquidity_sub_msg = SubMsg {
        id: LP_WITHDRAWAL_REPLY_ID,
        msg: withdraw_liquidity_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // TODO: burn liquidity tokens

    Ok(Response::new()
        .add_message(claim_unbonded_lps_msg)
        .add_submessage(withdraw_liquidity_sub_msg))
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

pub fn query_stake(deps: Deps, app: &AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_address(deps).unwrap().to_string(),
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    res.amount
}

fn claim_lps(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let modules = app.modules(deps);

    let msg: CosmosMsg = modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Claim { lp_token_name },
            },
        )
        .unwrap();

    return msg;
}
