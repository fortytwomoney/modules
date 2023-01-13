
use std::ops::Add;

use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::os::dex::{DexAction, DexExecuteMsg};

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::register::EXCHANGE;
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order,
    ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use cw_asset::AssetList;
use cw_utils::Duration;
use forty_two::autocompounder::{AutocompounderExecuteMsg, Cw20HookMsg};
use forty_two::cw_staking::{CwStakingAction, CwStakingExecuteMsg, CW_STAKING};

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_COMPOUND_REPLY_ID, LP_PROVISION_REPLY_ID,
    LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::state::{
    Claim, Config, CACHED_USER_ADDR, CLAIMS, CONFIG, LATEST_UNBONDING, PENDING_CLAIMS,
};

use super::helpers::{check_fee, cw20_total_supply, query_stake};

/// Handle the `AutocompounderExecuteMsg`s sent to this app.
pub fn execute_handler(
    deps: DepsMut,
    env: Env,
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
        AutocompounderExecuteMsg::Deposit { funds } => deposit(deps, info, env, app, funds),
        AutocompounderExecuteMsg::Withdraw {} => withdraw_claims(deps, app, env, info.sender),
        AutocompounderExecuteMsg::BatchUnbond {} => batch_unbond(deps, env, app),
        AutocompounderExecuteMsg::Compound {} => compound(deps, app),
    }
}

/// Update the application configuration.
pub fn update_fee_config(
    deps: DepsMut,
    msg_info: MessageInfo,
    app: AutocompounderApp,
    fee: Option<Decimal>,
    withdrawal: Option<Decimal>,
    deposit: Option<Decimal>,
) -> AutocompounderResult {
    app.admin.assert_admin(deps.as_ref(), &msg_info.sender)?;

    if let Some(fee) = fee {
        check_fee(fee)?;
        CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
            config.fees.performance = fee;
            Ok(config)
        })?;
    }

    if let Some(withdrawal) = withdrawal {
        check_fee(withdrawal)?;
        CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
            config.fees.withdrawal = withdrawal;

            Ok(config)
        })?;
    }

    if let Some(deposit) = deposit {
        check_fee(deposit)?;
        CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
            config.fees.deposit = deposit;
            Ok(config)
        })?;
    }

    Ok(Response::new().add_attribute("action", "update_fee_config"))
}

// This is the function that is called when the user wants to pool AND stake their funds
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
    let mut msgs = vec![];

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
    msgs.append(cw_20_transfer_msgs_res?.as_mut());

    // transfer received coins to the vault contract
    if !msg_info.funds.is_empty() {
        let bank = app.bank(deps.as_ref());
        msgs.push(bank.deposit_coins(msg_info.funds)?);
    }

    let modules = app.modules(deps.as_ref());
    let provide_liquidity_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.pool_data.dex,
            action: DexAction::ProvideLiquidity {
                assets: funds,
                max_spread: None,
            },
        },
    )?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: provide_liquidity_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &msg_info.sender)?;
    Ok(Response::new()
        .add_messages(msgs)
        .add_submessage(sub_msg)
        .add_attribute("action", "4T2/AC/Deposit"))
}

pub fn batch_unbond(deps: DepsMut, env: Env, app: AutocompounderApp) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // check if the cooldown period has passed
    check_unbonding_cooldown(&deps, &config, &env)?;

    let pending_claims = PENDING_CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(String, Uint128)>>>()?;

    let (total_lp_amount_to_unbond, total_vault_tokens_to_burn, updated_claims) =
        calculate_withdrawals(deps.as_ref(), &config, &app, pending_claims, env)?;

    // clear pending claims
    PENDING_CLAIMS.clear(deps.storage);
    // update claims
    updated_claims
        .into_iter()
        .try_for_each(|(addr, claims)| -> StdResult<()> {
            CLAIMS.save(deps.storage, addr, &claims)
        })?;

    let unstake_msg = unstake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        AssetEntry::from(LpToken::from(config.pool_data.clone())),
        total_lp_amount_to_unbond,
    );

    let burn_msg = get_burn_msg(&config.vault_token, total_vault_tokens_to_burn)?;

    Ok(Response::new()
        .add_messages(vec![unstake_msg, burn_msg])
        .add_attribute("action", "4T2/AC/UnbondBatch"))
}

/// Handles receiving CW20 messages
pub fn receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    app: AutocompounderApp,
    msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    // Withdraw fn can only be called by liquidity token
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.vault_token {
        return Err(AutocompounderError::SenderIsNotVaultToken {});
    }

    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, app, msg.sender, msg.amount),
    }
}

fn redeem(
    deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    sender: String,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    let _config = CONFIG.load(deps.storage)?;

    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &sender)?;

    if let Some(pending_claim) = PENDING_CLAIMS.may_load(deps.storage, sender.to_string())? {
        let new_pending_claim = pending_claim
            .checked_add(amount_of_vault_tokens_to_be_burned)
            .unwrap();
        PENDING_CLAIMS.save(deps.storage, sender.to_string(), &new_pending_claim)?;
    } else {
        PENDING_CLAIMS.save(
            deps.storage,
            sender.to_string(),
            &amount_of_vault_tokens_to_be_burned,
        )?;
    }

    Ok(Response::new().add_attribute("action", "4T2/AC/Register_pre_claim"))
}

fn compound(deps: DepsMut, app: AutocompounderApp) -> AutocompounderResult {
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

/// withdraw all matured claims for a user
pub fn withdraw_claims(
    deps: DepsMut,
    app: AutocompounderApp,
    env: Env,
    address: Addr,
) -> AutocompounderResult {
    CACHED_USER_ADDR.save(deps.storage, &address)?;
    let config = CONFIG.load(deps.storage)?;
    let Some(claims) = CLAIMS.may_load(deps.storage, address.to_string())? else {
        return Err(AutocompounderError::NoMaturedClaims {});
    };

    // 1) get all matured claims for user
    let mut ongoing_claims: Vec<Claim> = vec![];
    let mut matured_claims: Vec<Claim> = vec![];
    claims.into_iter().for_each(|claim| {
        if claim.unbonding_timestamp.is_expired(&env.block) {
            matured_claims.push(claim);
        } else {
            ongoing_claims.push(claim);
        }
    });

    if matured_claims.is_empty() {
        return Err(AutocompounderError::NoMaturedClaims {});
    }

    CLAIMS.save(deps.storage, address.to_string(), &ongoing_claims)?;

    // 2) sum up all matured claims
    let lp_tokens_to_withdraw: Uint128 =
        matured_claims.iter().fold(Uint128::zero(), |acc, claim| {
            acc + claim.amount_of_lp_tokens_to_unbond
        });

    // 3) withdraw lp tokens
    let modules = app.modules(deps.as_ref());
    let swap_msg: CosmosMsg = modules.api_request(
        EXCHANGE,
        DexExecuteMsg {
            dex: config.pool_data.dex,
            action: DexAction::WithdrawLiquidity {
                lp_token: config.liquidity_token.to_string().into(),
                amount: lp_tokens_to_withdraw,
            },
        },
    )?;
    let sub_msg = SubMsg::reply_on_success(swap_msg, LP_WITHDRAWAL_REPLY_ID);

    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "4T2/AC/Withdraw_claims")
        .add_attribute("lp_tokens_to_withdraw", lp_tokens_to_withdraw.to_string()))
}

#[allow(clippy::type_complexity)]
/// Calculates the amount the total amount of lp tokens to unbond and vault tokens to burn
fn calculate_withdrawals(
    deps: Deps,
    config: &Config,
    app: &AutocompounderApp,
    pending_claims: Vec<(String, Uint128)>,
    env: Env,
) -> Result<(Uint128, Uint128, Vec<(String, Vec<Claim>)>), AutocompounderError> {
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data.clone()));
    let unbonding_timestamp = config
        .bonding_period
        .unwrap_or(Duration::Height(0))
        .after(&env.block);

    let mut total_lp_amount_to_unbond = Uint128::from(0u128);
    let mut total_vault_tokens_to_burn = Uint128::from(0u128);

    // 1) get the total supply of Vault token
    let vault_tokens_total_supply = cw20_total_supply(deps, config)?;

    // 2) get total staked lp token
    let total_lp_tokens_staked_in_vault =
        query_stake(deps, app, config.pool_data.dex.clone(), lp_token)?;

    let mut updated_claims: Vec<(String, Vec<Claim>)> = vec![];
    for pending_claim in pending_claims {
        let user_address = pending_claim.0;
        let user_amount_of_vault_tokens_to_be_burned = pending_claim.1;

        let user_lp_tokens_withdraw_amount = Decimal::from_ratio(
            user_amount_of_vault_tokens_to_be_burned,
            vault_tokens_total_supply,
        ) * total_lp_tokens_staked_in_vault;

        total_lp_amount_to_unbond = total_lp_amount_to_unbond
            .checked_add(user_lp_tokens_withdraw_amount)
            .unwrap();

        total_vault_tokens_to_burn = total_vault_tokens_to_burn
            .checked_add(user_amount_of_vault_tokens_to_be_burned)
            .unwrap();

        // sets the unbonding timestamp to the current block height + bonding period
        let new_claim = Claim {
            unbonding_timestamp,
            amount_of_vault_tokens_to_burn: user_amount_of_vault_tokens_to_be_burned,
            amount_of_lp_tokens_to_unbond: user_lp_tokens_withdraw_amount,
        };

        if let Some(mut existent_claims) = CLAIMS.may_load(deps.storage, user_address.clone())? {
            existent_claims.push(new_claim);
            updated_claims.push((user_address, existent_claims))
        } else {
            updated_claims.push((user_address, vec![new_claim]))
        }
    }
    Ok((
        total_lp_amount_to_unbond,
        total_vault_tokens_to_burn,
        updated_claims,
    ))
}

/// Checks if the unbonding cooldown period for batch unbonding has passed or not.
fn check_unbonding_cooldown(
    deps: &DepsMut,
    config: &crate::state::Config,
    env: &Env,
) -> Result<(), AutocompounderError> {
    let latest_unbonding = LATEST_UNBONDING.load(deps.storage)?;
    if let Some(min_cooldown) = config.min_unbonding_cooldown {
        if latest_unbonding.add(min_cooldown)?.is_expired(&env.block) {
            return Err(AutocompounderError::UnbondingCooldownNotExpired {
                min_cooldown,
                latest_unbonding,
            });
        }
    };
    Ok(())
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

fn get_burn_msg(contract: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::Burn { amount };
    Ok(WasmMsg::Execute {
        contract_addr: contract.to_string(),
        msg: to_binary(&msg)?,
        funds: vec![],
    }
    .into())
}

fn unstake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
) -> CosmosMsg {
    let modules = app.modules(deps);

    modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Unstake {
                    staking_token: AnsAsset::new(lp_token_name, amount),
                },
            },
        )
        .unwrap()
}
