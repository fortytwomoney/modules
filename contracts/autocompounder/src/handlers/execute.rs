use super::convert_to_shares;
use super::helpers::{
    check_fee, convert_to_assets, cw20_total_supply, mint_vault_tokens, query_stake,
    stake_lp_tokens, transfer_to_msgs,
};
use super::instantiate::{get_unbonding_period_and_min_unbonding_cooldown, query_staking_info};

use abstract_core::objects::AnsEntryConvertor;
use abstract_sdk::{AccountAction, AdapterInterface};

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_COMPOUND_REPLY_ID, LP_PROVISION_REPLY_ID,
    LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::msg::{AutocompounderExecuteMsg, BondingPeriodSelector, Cw20HookMsg};
use crate::state::{
    Claim, Config, FeeConfig, CACHED_ASSETS, CACHED_USER_ADDR, CLAIMS, CONFIG, DEFAULT_BATCH_SIZE,
    DEFAULT_MAX_SPREAD, FEE_CONFIG, LATEST_UNBONDING, MAX_BATCH_SIZE, PENDING_CLAIMS,
};
use abstract_cw_staking::msg::{StakingAction, StakingExecuteMsg};
use abstract_cw_staking::CW_STAKING;
use abstract_dex_adapter::api::DexInterface;
use abstract_sdk::Execution;
use abstract_sdk::{
    core::objects::{AnsAsset, AssetEntry, LpToken},
    features::{AbstractNameService, AccountIdentification},
    Resolve, TransferInterface,
};
use abstract_sdk::{features::AbstractResponse, AbstractSdkError};
use cosmwasm_std::{
    from_binary, wasm_execute, Addr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, ReplyOn, Response, StdResult, SubMsg, Uint128,
};
use cw20::Cw20ReceiveMsg;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetList};
use cw_storage_plus::Bound;
use cw_utils::Duration;
use std::ops::Add;

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
            fee_collector_addr,
        } => update_fee_config(
            deps,
            info,
            app,
            performance,
            withdrawal,
            deposit,
            fee_collector_addr,
        ),
        AutocompounderExecuteMsg::Deposit { funds, max_spread } => {
            deposit(deps, info, env, app, funds, max_spread)
        }
        AutocompounderExecuteMsg::DepositLp { lp_token, receiver } => {
            deposit_lp(deps, info, env, app, lp_token, receiver)
        }
        AutocompounderExecuteMsg::Withdraw {} => withdraw_claims(deps, app, env, info.sender),
        AutocompounderExecuteMsg::BatchUnbond { start_after, limit } => {
            batch_unbond(deps, env, app, start_after, limit)
        }
        AutocompounderExecuteMsg::Compound {} => compound(deps, app),
        AutocompounderExecuteMsg::UpdateStakingConfig {
            preferred_bonding_period,
        } => update_staking_config(deps, app, info, preferred_bonding_period),
    }
}

pub fn update_staking_config(
    deps: DepsMut,
    app: AutocompounderApp,
    info: MessageInfo,
    preferred_bonding_period: BondingPeriodSelector,
) -> AutocompounderResult {
    app.admin.assert_admin(deps.as_ref(), &info.sender)?;

    let mut config = CONFIG.load(deps.storage)?;

    let lp_token = LpToken {
        dex: config.pool_data.dex.clone(),
        assets: config.pool_data.assets.clone(),
    };

    // get staking info
    let staking_info = query_staking_info(
        deps.as_ref(),
        &app,
        AnsEntryConvertor::new(lp_token.clone()).asset_entry(),
        lp_token.dex,
    )?;
    let (unbonding_period, min_unbonding_cooldown) =
        get_unbonding_period_and_min_unbonding_cooldown(staking_info, preferred_bonding_period)?;

    config.unbonding_period = unbonding_period;
    config.min_unbonding_cooldown = min_unbonding_cooldown;

    CONFIG.save(deps.storage, &config)?;

    Ok(app.custom_tag_response(
        Response::new(),
        "update_config_with_staking_contract_data",
        vec![("4t2", "/AC/UpdateConfigWithStakingContractData")],
    ))
}

/// Update the application configuration.
pub fn update_fee_config(
    deps: DepsMut,
    msg_info: MessageInfo,
    app: AutocompounderApp,
    fee: Option<Decimal>,
    withdrawal: Option<Decimal>,
    deposit: Option<Decimal>,
    fee_collector_addr: Option<String>,
) -> AutocompounderResult {
    app.admin.assert_admin(deps.as_ref(), &msg_info.sender)?;

    let mut config = FEE_CONFIG.load(deps.storage)?;
    let mut updates = vec![];

    if let Some(fee) = fee {
        check_fee(fee)?;
        updates.push(("performance", fee.to_string()));
        config.performance = fee;
    }

    if let Some(withdrawal) = withdrawal {
        check_fee(withdrawal)?;
        updates.push(("withdrawal", withdrawal.to_string()));
        config.withdrawal = withdrawal;
    }

    if let Some(deposit) = deposit {
        check_fee(deposit)?;
        updates.push(("deposit", deposit.to_string()));
        config.deposit = deposit;
    }

    if let Some(fee_collector_addr) = fee_collector_addr {
        let fee_collector_addr_validated = deps.api.addr_validate(&fee_collector_addr)?;
        updates.push((
            "fee_collector_addr",
            fee_collector_addr_validated.to_string(),
        ));
        config.fee_collector_addr = fee_collector_addr_validated;
    }

    FEE_CONFIG.save(deps.storage, &config)?;

    Ok(app.custom_tag_response(Response::new(), "update_fee_config", updates))
}

// This is the function that is called when the user wants to pool AND stake their funds
pub fn deposit(
    deps: DepsMut,
    msg_info: MessageInfo,
    _env: Env,
    app: AutocompounderApp,
    mut funds: Vec<AnsAsset>,
    max_spread: Option<Decimal>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;

    let ans_host = app.ans_host(deps.as_ref())?;
    let dex = app.dex(deps.as_ref(), config.pool_data.dex);
    let resolved_pool_assets = config.pool_data.assets.resolve(&deps.querier, &ans_host)?;
    let _lptoken = config.liquidity_token.clone();

    let mut messages = vec![];
    let mut submessages = vec![];

    // check if sent coins are only correct coins
    for Coin { denom, amount: _ } in msg_info.funds.iter() {
        if !resolved_pool_assets.contains(&AssetInfo::Native(denom.clone())) {
            return Err(AutocompounderError::CoinNotInPool {
                denom: denom.clone(),
            });
        }
    }

    // check if all the assets in funds are present in the pool
    for asset in funds.iter() {
        if !config.pool_data.assets.contains(&asset.name) {
            return Err(AutocompounderError::AssetNotInPool {
                asset: asset.name.to_string(),
            });
        }
    }

    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits
        .deduct_many(&msg_info.funds.clone().into())?
        .purge();

    // if there is only one asset, we need to add the other asset too, but with zero amount
    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>, AbstractSdkError> = claimed_deposits
        .into_iter()
        .map(|asset| {
            // transfer cw20 tokens to the Account
            // will fail if allowance is not set or if some other assets are sent
            Ok(asset.transfer_from_msg(&msg_info.sender, app.proxy_address(deps.as_ref())?)?)
        })
        .collect();
    messages.append(cw_20_transfer_msgs_res?.as_mut());

    let mut account_msgs = AccountAction::new();
    // transfer received coins to the Account
    if !msg_info.funds.is_empty() {
        let bank = app.bank(deps.as_ref());
        messages.extend(bank.deposit(msg_info.funds)?);
    }

    // deduct deposit fee
    if !fee_config.deposit.is_zero() {
        let mut fees = vec![];
        funds = funds
            .into_iter()
            .map(|mut asset| {
                let fee = asset.amount * fee_config.deposit;
                let fee_asset = AnsAsset::new(asset.name.clone(), fee);
                asset.amount -= fee;
                if !fee.is_zero() {
                    fees.push(fee_asset);
                }
                asset
            })
            .collect::<Vec<_>>();

        // 3) Send fees to the feecollector
        if !fees.is_empty() {
            let transfer_msg = app
                .bank(deps.as_ref())
                .transfer(fees, &fee_config.fee_collector_addr)?;
            account_msgs.merge(transfer_msg);
        }
    }

    // Add the other asset if there is only one asset for liquidity provision
    let funds = if funds.len() == 1 {
        let mut funds = funds;
        config.pool_data.assets.iter().for_each(|asset| {
            if !funds[0].name.eq(asset) {
                funds.push(AnsAsset::new(asset.clone(), 0u128))
            }
        });
        funds
    } else {
        funds
    };

    let provide_liquidity_msg: CosmosMsg = dex.provide_liquidity(
        funds,
        Some(max_spread.unwrap_or_else(|| Decimal::percent(DEFAULT_MAX_SPREAD.into()))),
    )?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: provide_liquidity_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };
    submessages.push(sub_msg);

    // save the user address to the cache for later use in reply
    // CACHED_FEE_AMOUNT.save(deps.storage, &current_fee_balance)?;
    CACHED_USER_ADDR.save(deps.storage, &msg_info.sender)?;

    let mut response = Response::new()
        .add_messages(messages)
        .add_submessages(submessages);

    if !account_msgs.messages().is_empty() {
        response = response.add_message(app.executor(deps.as_ref()).execute(vec![account_msgs])?);
    }
    Ok(app.custom_tag_response(response, "deposit", vec![("4t2", "/AC/Deposit")]))
}

fn deposit_lp(
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    app: AutocompounderApp,
    lp_asset: AnsAsset,
    receiver: Option<Addr>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let ans = app.name_service(deps.as_ref());
    let lp_token = ans.query(&lp_asset)?;
    let lp_asset_entry = lp_asset.name.clone();
    let receiver = receiver.unwrap_or(info.sender.clone());

    if lp_token.info != config.liquidity_token {
        return Err(AutocompounderError::SenderIsNotLpToken {});
    };

    // let lp_token = AnsEntryConvertor::new(config.pool_data.clone()).lp_token();

    // transfer the asset to the proxy contract
    let transfer_msg = transfer_token(lp_token, info, &app, deps.as_ref())?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_asset_entry,
        config.unbonding_period,
    )?;

    let (lp_asset, fee_asset) = deduct_fee(lp_asset, fee_config.deposit);
    let fee_msgs = transfer_to_msgs(
        &app,
        deps.as_ref(),
        fee_asset,
        fee_config.fee_collector_addr,
    )?;

    let current_vault_supply = cw20_total_supply(deps.as_ref(), &config)?;
    let mint_amount = convert_to_shares(lp_asset.amount, staked_lp, current_vault_supply);
    if mint_amount.is_zero() {
        return Err(AutocompounderError::ZeroMintAmount {});
    }

    let mint_msg = mint_vault_tokens(&config, receiver, mint_amount)?;
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        lp_asset,
        config.unbonding_period,
    )?;

    let res = Response::new()
        .add_message(transfer_msg)
        .add_messages(vec![mint_msg, stake_msg])
        .add_messages(fee_msgs);

    Ok(app.custom_tag_response(res, "deposit-lp", vec![("4t2", "/AC/DepositLP")]))
}

fn deduct_fee(lp_asset: AnsAsset, fee: Decimal) -> (AnsAsset, AnsAsset) {
    let mut fee_asset = AnsAsset::new(lp_asset.name.clone(), Uint128::zero());
    let mut lp_asset = lp_asset;
    if fee.is_zero() {
        (lp_asset, fee_asset)
    } else {
        let fee_amount = lp_asset.amount * fee;
        fee_asset.amount = fee_amount;
        lp_asset.amount -= fee_amount;
        (lp_asset, fee_asset)
    }
}

/// Transfer lp token to the proxy contract whether it is a cw20 or native token. Returns CosmosMsg
/// For cw20 tokens, it will call transfer_from and it needs a allowance to be set, otherwhise the execution will error.
fn transfer_token(
    lp_token: Asset,
    info: MessageInfo,
    app: &AutocompounderApp,
    deps: Deps,
) -> Result<CosmosMsg, AutocompounderError> {
    match lp_token.info.clone() {
        AssetInfoBase::Cw20(_addr) => Asset::cw20(_addr, lp_token.amount)
            .transfer_from_msg(info.sender, app.proxy_address(deps)?)
            .map_err(|e| e.into()),
        AssetInfoBase::Native(_denom) => Ok(app.bank(deps).deposit(vec![lp_token])?.swap_remove(0)),
        _ => Err(AutocompounderError::AssetError(
            cw_asset::AssetError::InvalidAssetFormat {
                received: lp_token.to_string(),
            },
        )),
    }
}

pub fn batch_unbond(
    deps: DepsMut,
    env: Env,
    app: AutocompounderApp,
    start_after: Option<String>,
    limit: Option<u32>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    if config.unbonding_period.is_none() {
        return Err(AutocompounderError::UnbondingNotEnabled {});
    }

    // check if the cooldown period has passed
    check_unbonding_cooldown(&deps, &config, &env)?;
    LATEST_UNBONDING.save(deps.storage, &cw_utils::Expiration::AtTime(env.block.time))?;

    let limit = limit.unwrap_or(DEFAULT_BATCH_SIZE).min(MAX_BATCH_SIZE) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into_bytes()));
    let pending_claims = PENDING_CLAIMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect::<StdResult<Vec<(String, Uint128)>>>()?;

    let (total_lp_amount_to_unbond, total_vault_tokens_to_burn, updated_claims) =
        calculate_withdrawals(
            deps.as_ref(),
            &config,
            &fee_config,
            &app,
            pending_claims.clone(),
            env,
        )?;

    // clear pending claims
    for claim in pending_claims.iter() {
        PENDING_CLAIMS.remove(deps.storage, claim.0.clone());
    }
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
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data).lp_token()).asset_entry(),
        total_lp_amount_to_unbond,
        config.unbonding_period,
    );

    let burn_msg = get_burn_msg(&config.vault_token, total_vault_tokens_to_burn)?;

    let response = Response::new().add_messages(vec![unstake_msg, burn_msg]);
    Ok(app.custom_tag_response(response, "batch_unbond", vec![("4t2", "AC/UnbondBatch")]))
}

/// Handles receiving CW20 messages
pub fn receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    app: AutocompounderApp,
    msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    if msg.amount.is_zero() {
        return Err(AutocompounderError::ZeroDepositAmount {});
    }
    // Withdraw fn can only be called by liquidity token or the lp token
    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, app, info.sender, msg.sender, msg.amount),
    }
}

fn redeem(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    cw20_sender: Addr,
    sender: String,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // check if the cw20 sender is the vault token
    let config = CONFIG.load(deps.storage)?;
    if cw20_sender != config.vault_token {
        return Err(AutocompounderError::SenderIsNotVaultToken {});
    }

    if config.unbonding_period.is_none() {
        redeem_without_bonding_period(
            deps,
            &sender,
            config,
            &app,
            amount_of_vault_tokens_to_be_burned,
        )
    } else {
        register_pre_claim(deps, sender, amount_of_vault_tokens_to_be_burned)?;

        Ok(app.custom_tag_response(
            Response::new(),
            "redeem",
            vec![("4t2", "AC/Register_pre_claim")],
        ))
    }
}

/// Registers a pending-claim when Redeem is called for a pool with bonding period.
/// This will store the claim of te user and add it to any pending claims.
/// The claim will be processed in the next batch unbonding
fn register_pre_claim(
    deps: DepsMut,
    sender: Addr,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> Result<(), AutocompounderError> {
    // if bonding period is set, we need to register the user's pending claim, that will be processed in the next batch unbonding
    if let Some(pending_claim) = PENDING_CLAIMS.may_load(deps.storage, sender.to_string())? {
        let new_pending_claim = pending_claim
            .checked_add(amount_of_vault_tokens_to_be_burned)
            .unwrap();
        PENDING_CLAIMS.save(deps.storage, sender.to_string(), &new_pending_claim)?;
    // if not, we just store a new claim
    } else {
        PENDING_CLAIMS.save(
            deps.storage,
            sender.to_string(),
            &amount_of_vault_tokens_to_be_burned,
        )?;
    }

    Ok(())
}

/// Redeems the vault tokens without a bonding period.
/// This will unstake the lp tokens, burn the vault tokens, withdraw the underlying assets and send them to the user
fn redeem_without_bonding_period(
    deps: DepsMut,
    sender: &Addr,
    config: Config,
    app: &AutocompounderApp,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> Result<Response, AutocompounderError> {
    let fee_config = FEE_CONFIG.load(deps.storage)?;

    // save the user address and the assets owned by the contract to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, sender)?;
    let owned_assets = app.bank(deps.as_ref()).balances(&config.pool_data.assets)?;
    owned_assets.into_iter().try_for_each(|asset| {
        CACHED_ASSETS
            // CACHED_ASSETS are saved with the key being cwasset::asset:AssetInfo.to_string()
            .save(deps.storage, asset.info.to_string(), &asset.amount)
            .map_err(AutocompounderError::Std)
    })?;

    // 1) get the total supply of Vault token
    let total_supply_vault = cw20_total_supply(deps.as_ref(), &config)?;
    let lp_token = AnsEntryConvertor::new(config.pool_data.clone()).lp_token();

    // 2) get total staked lp token
    let total_lp_tokens_staked_in_vault = query_stake(
        deps.as_ref(),
        app,
        config.pool_data.dex.clone(),
        AnsEntryConvertor::new(lp_token).asset_entry(),
        None,
    )?;

    let lp_tokens_withdraw_amount = convert_to_assets(
        amount_of_vault_tokens_to_be_burned,
        total_lp_tokens_staked_in_vault,
        total_supply_vault,
    );

    // Substract withdrawal fee from the amount of lp tokens allocated to the user
    let lp_tokens_withdraw_amount =
        lp_tokens_withdraw_amount.checked_sub(lp_tokens_withdraw_amount * fee_config.withdrawal)?;

    // unstake lp tokens
    let unstake_msg = unstake_lp_tokens(
        deps.as_ref(),
        app,
        config.pool_data.dex.clone(),
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data.clone()).lp_token())
            .asset_entry(),
        lp_tokens_withdraw_amount,
        None,
    );
    let burn_msg = get_burn_msg(&config.vault_token, amount_of_vault_tokens_to_be_burned)?;

    // 3) withdraw lp tokens
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());
    let withdraw_msg: CosmosMsg = dex.withdraw_liquidity(
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data).lp_token()).asset_entry(),
        lp_tokens_withdraw_amount,
    )?;
    let sub_msg = SubMsg::reply_on_success(withdraw_msg, LP_WITHDRAWAL_REPLY_ID);

    let response = Response::new()
        .add_message(unstake_msg)
        .add_message(burn_msg)
        .add_submessage(sub_msg);
    Ok(app.custom_tag_response(
        response,
        "redeem",
        vec![
            (
                "vault_token_burn_amount",
                &amount_of_vault_tokens_to_be_burned.to_string(),
            ),
            (
                "lp_token_withdraw_amount",
                &lp_tokens_withdraw_amount.to_string(),
            ),
        ],
    ))
}

fn compound(deps: DepsMut, app: AutocompounderApp) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // 1) Claim rewards from staking contract
    let claim_msg = claim_lp_rewards(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data).lp_token()).asset_entry(),
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

    let response = Response::new().add_submessage(claim_submsg);
    Ok(app.tag_response(response, "compound"))
}

/// withdraw all matured claims for a user
pub fn withdraw_claims(
    deps: DepsMut,
    app: AutocompounderApp,
    env: Env,
    sender: Addr,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let pool_assets = config.pool_data.assets.clone();
    if config.unbonding_period.is_none() {
        return Err(AutocompounderError::UnbondingNotEnabled {});
    }

    // cache assets and address for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &sender)?;
    let owned_assets = app.bank(deps.as_ref()).balances(&pool_assets)?;
    owned_assets
        .into_iter()
        .enumerate()
        .for_each(|(_i, asset)| {
            CACHED_ASSETS
                .save(deps.storage, asset.info.to_string(), &asset.amount)
                .unwrap();
        });

    let Some(claims) = CLAIMS.may_load(deps.storage, sender.to_string())? else {
        return Err(AutocompounderError::NoClaims {});
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

    CLAIMS.save(deps.storage, sender.to_string(), &ongoing_claims)?;

    // 2) sum up all matured claims
    let lp_tokens_to_withdraw: Uint128 =
        matured_claims.iter().fold(Uint128::zero(), |acc, claim| {
            acc + claim.amount_of_lp_tokens_to_unbond
        });

    // 3.1) claim all matured claims from staking contract
    let claim_msg = claim_unbonded_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data.clone()).lp_token())
            .asset_entry(),
    );

    // 3) withdraw lp tokens
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());
    let withdraw_msg: CosmosMsg = dex.withdraw_liquidity(
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data).lp_token()).asset_entry(),
        lp_tokens_to_withdraw,
    )?;
    let sub_msg = SubMsg::reply_on_success(withdraw_msg, LP_WITHDRAWAL_REPLY_ID);

    let response = Response::new()
        .add_message(claim_msg)
        .add_submessage(sub_msg);
    Ok(app.custom_tag_response(
        response,
        "withdraw_claims",
        vec![
            ("4t2", "AC/Withdraw_claims".to_string()),
            ("lp_tokens_to_withdraw", lp_tokens_to_withdraw.to_string()),
        ],
    ))
}

#[allow(clippy::type_complexity)]
/// Calculates the amount the total amount of lp tokens to unbond and vault tokens to burn
fn calculate_withdrawals(
    deps: Deps,
    config: &Config,
    fee_config: &FeeConfig,
    app: &AutocompounderApp,
    pending_claims: Vec<(String, Uint128)>,
    env: Env,
) -> Result<(Uint128, Uint128, Vec<(String, Vec<Claim>)>), AutocompounderError> {
    let lp_token =
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data.clone()).lp_token())
            .asset_entry();
    let unbonding_timestamp = config
        .unbonding_period
        .ok_or(AutocompounderError::UnbondingNotEnabled {})
        .unwrap_or(Duration::Height(0))
        .after(&env.block);

    let mut total_lp_amount_to_unbond = Uint128::from(0u128);
    let mut total_vault_tokens_to_burn = Uint128::from(0u128);

    // 1) get the total supply of Vault token
    let vault_tokens_total_supply = cw20_total_supply(deps, config)?;

    // 2) get total staked lp token
    let total_lp_tokens_staked_in_vault = query_stake(
        deps,
        app,
        config.pool_data.dex.clone(),
        lp_token,
        config.unbonding_period,
    )?;

    let mut updated_claims: Vec<(String, Vec<Claim>)> = vec![];
    for pending_claim in pending_claims {
        let user_address = pending_claim.0;
        let user_amount_of_vault_tokens_to_be_burned = pending_claim.1;

        let user_lp_tokens_withdraw_amount = convert_to_assets(
            user_amount_of_vault_tokens_to_be_burned,
            total_lp_tokens_staked_in_vault,
            vault_tokens_total_supply,
        );

        // substract withdrawal fees from the amount of lp tokens to unbond
        let user_lp_tokens_withdraw_amount = user_lp_tokens_withdraw_amount
            .checked_sub(user_lp_tokens_withdraw_amount * fee_config.withdrawal)?;

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
    config: &Config,
    env: &Env,
) -> Result<(), AutocompounderError> {
    let latest_unbonding = LATEST_UNBONDING.may_load(deps.storage)?;
    if let Some(latest_unbonding) = latest_unbonding {
        if let Some(min_cooldown) = config.min_unbonding_cooldown {
            if !latest_unbonding.add(min_cooldown)?.is_expired(&env.block) {
                return Err(AutocompounderError::UnbondingCooldownNotExpired {
                    min_cooldown,
                    latest_unbonding,
                });
            }
        };
    }
    Ok(())
}

/// Creates the message to claim the rewards from the staking api
fn claim_lp_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let adapters = app.adapters(deps);

    adapters
        .request(
            CW_STAKING,
            StakingExecuteMsg {
                provider,
                action: StakingAction::ClaimRewards {
                    asset: lp_token_name,
                },
            },
        )
        .unwrap()
}

/// Creates the message to claim the unbonded tokens from the staking api
fn claim_unbonded_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let adapters = app.adapters(deps);

    adapters
        .request(
            CW_STAKING,
            StakingExecuteMsg {
                provider,
                action: StakingAction::Claim {
                    asset: lp_token_name,
                },
            },
        )
        .unwrap()
}

/// Creates the message to burn tokens from contract
fn get_burn_msg(contract: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::Burn { amount };

    Ok(wasm_execute(contract.to_string(), &msg, vec![])?.into())
}

/// Creates the the message to unstake lp tokens from the staking api
fn unstake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
    unbonding_period: Option<Duration>,
) -> CosmosMsg {
    let adapters = app.adapters(deps);

    adapters
        .request(
            CW_STAKING,
            StakingExecuteMsg {
                provider,
                action: StakingAction::Unstake {
                    asset: AnsAsset::new(lp_token_name, amount),
                    unbonding_period,
                },
            },
        )
        .unwrap()
}

#[cfg(test)]
mod test {
    use super::{redeem_without_bonding_period, *};

    use crate::handlers::helpers::test_helpers::min_cooldown_config;
    use crate::msg::ExecuteMsg;
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_init};

    use abstract_sdk::base::ExecuteEndpoint;
    use abstract_testing::prelude::{TEST_MANAGER, TEST_PROXY};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Attribute, Coin};
    use cw_controllers::AdminError;
    use cw_utils::Expiration;
    use speculoos::vec::VecAssertions;
    use speculoos::{assert_that, result::ResultAssertions};

    fn execute_as(
        deps: DepsMut,
        sender: &str,
        msg: impl Into<ExecuteMsg>,
        funds: &[Coin],
    ) -> Result<Response, AutocompounderError> {
        let info = mock_info(sender, funds);
        AUTOCOMPOUNDER_APP.execute(deps, mock_env(), info, msg.into())
    }

    fn execute_as_manager(
        deps: DepsMut,
        msg: impl Into<ExecuteMsg>,
    ) -> Result<Response, AutocompounderError> {
        execute_as(deps, TEST_MANAGER, msg, &[])
    }

    #[test]
    fn test_redeem_without_bonding_period() -> anyhow::Result<()> {
        let mut deps = app_init(false);
        let config = min_cooldown_config(None);
        let sender = Addr::unchecked("sender");
        let amount = Uint128::new(100);

        let err = CACHED_ASSETS.load(&deps.storage, "native:eur".to_string());
        assert_that!(err).is_err();

        // 1. set up the balances(1000eur, 1000usd) of the proxy contract in the bank
        deps.querier.update_balance(
            TEST_PROXY,
            vec![
                Coin {
                    denom: "eur".to_string(),
                    amount: Uint128::new(1000),
                },
                Coin {
                    denom: "usd".to_string(),
                    amount: Uint128::new(1000),
                },
            ],
        );

        let response = redeem_without_bonding_period(
            deps.as_mut(),
            &sender,
            config.clone(),
            &AUTOCOMPOUNDER_APP,
            amount,
        )?;

        // The sender addr should be cached
        assert_that!(CACHED_USER_ADDR.load(&deps.storage)?).is_equal_to(sender);

        // The contract should not own assets at this point and should have stored them correctly
        let cached_assets: Vec<(String, Uint128)> = CACHED_ASSETS
            .range(&deps.storage, None, None, Order::Ascending)
            .map(|x| x.unwrap())
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_that!(cached_assets).has_length(2);
        assert_that!(cached_assets[0]).is_equal_to(("native:eur".to_string(), 1000u128.into()));
        assert_that!(cached_assets[1]).is_equal_to(("native:usd".to_string(), 1000u128.into()));

        // The contract should have sent the correct messages
        assert_that!(response.messages).has_length(3);
        assert_that!(response.messages[0].msg).is_equal_to(unstake_lp_tokens(
            deps.as_ref(),
            &AUTOCOMPOUNDER_APP,
            config.pool_data.dex.clone(),
            AssetEntry::new("wyndex/eur,usd"),
            10u128.into(),
            None,
        ));

        let abstract_attributes = response.events[0].attributes.clone();
        assert_that!(abstract_attributes[2])
            .is_equal_to(Attribute::new("vault_token_burn_amount", "100"));
        assert_that!(abstract_attributes[3])
            .is_equal_to(Attribute::new("lp_token_withdraw_amount", "10"));
        Ok(())
    }

    mod fee_config {
        use speculoos::{assert_that, result::ResultAssertions};

        use crate::test_common::app_init;

        use super::*;

        #[test]
        fn only_admin() -> anyhow::Result<()> {
            let mut deps = app_init(false);
            let msg = AutocompounderExecuteMsg::UpdateFeeConfig {
                performance: None,
                deposit: Some(Decimal::percent(1)),
                withdrawal: None,
                fee_collector_addr: None,
            };

            let resp = execute_as(deps.as_mut(), "not_mananger", msg.clone(), &[]);
            assert_that!(resp)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::Admin(AdminError::NotAdmin {})));

            // successfully update the fee config as the manager (also the admin)
            execute_as_manager(deps.as_mut(), msg)?;

            let new_fee = FEE_CONFIG.load(deps.as_ref().storage)?;

            assert_that!(new_fee.deposit).is_equal_to(Decimal::percent(1));
            Ok(())
        }
        #[test]
        fn cannot_set_fee_above_or_equal_1() -> anyhow::Result<()> {
            let mut deps = app_init(false);
            let msg = AutocompounderExecuteMsg::UpdateFeeConfig {
                performance: None,
                deposit: Some(Decimal::one()),
                withdrawal: None,
                fee_collector_addr: None,
            };

            let resp = execute_as_manager(deps.as_mut(), msg);
            assert_that!(resp)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::InvalidFee {}));
            Ok(())
        }

        #[test]
        fn update_fee_collector() -> anyhow::Result<()> {
            const NEW_FEE_COLLECTOR: &str = "new_fee_collector_addr";
            let mut deps = app_init(false);
            let msg = AutocompounderExecuteMsg::UpdateFeeConfig {
                performance: None,
                deposit: None,
                withdrawal: None,
                fee_collector_addr: Some(NEW_FEE_COLLECTOR.to_string()),
            };

            let resp = execute_as(deps.as_mut(), "not_mananger", msg.clone(), &[]);
            assert_that!(resp)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::Admin(AdminError::NotAdmin {})));

            let resp = execute_as_manager(deps.as_mut(), msg);
            assert_that!(resp).is_ok();

            let new_fee_config = FEE_CONFIG.load(deps.as_ref().storage)?;
            assert_that!(new_fee_config.fee_collector_addr.to_string())
                .is_equal_to(NEW_FEE_COLLECTOR.to_string());

            Ok(())
        }
    }

    mod config {

        use cw_controllers::AdminError;
        use speculoos::assert_that;

        use crate::error::AutocompounderError;
        use crate::msg::{AutocompounderExecuteMsg, BondingPeriodSelector};
        use crate::state::{Config, CONFIG};
        use crate::test_common::app_init;

        use super::*;

        #[test]
        fn update_staking_config_only_admin() -> anyhow::Result<()> {
            let mut deps = app_init(true);
            let msg = AutocompounderExecuteMsg::UpdateStakingConfig {
                preferred_bonding_period: BondingPeriodSelector::Longest,
            };

            let resp = execute_as(deps.as_mut(), "not_mananger", msg.clone(), &[]);
            assert_that!(resp)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::Admin(AdminError::NotAdmin {})));

            // successfully update the fee config as the manager (also the admin)
            execute_as_manager(deps.as_mut(), msg)?;

            let new_config: Config = CONFIG.load(deps.as_ref().storage)?;

            assert_that!(new_config.unbonding_period).is_equal_to(Some(Duration::Time(7200)));
            Ok(())
        }
    }

    #[test]
    fn cannot_batch_unbond_if_unbonding_not_enabled() -> anyhow::Result<()> {
        let mut deps = app_init(false);
        let msg = AutocompounderExecuteMsg::BatchUnbond {
            start_after: None,
            limit: None,
        };
        let resp = execute_as_manager(deps.as_mut(), msg);
        assert_that!(resp)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::UnbondingNotEnabled {}));
        Ok(())
    }

    #[test]
    fn cannot_send_wrong_coins() -> anyhow::Result<()> {
        let mut deps = app_init(true);
        let msg = AutocompounderExecuteMsg::Deposit {
            funds: vec![AnsAsset::new("eur", Uint128::one())],
            max_spread: None,
        };

        let wrong_coin = "juno".to_string();
        let resp = execute_as(
            deps.as_mut(),
            "user",
            msg,
            &[Coin::new(1u128, "eur"), Coin::new(1u128, wrong_coin)],
        );
        assert_that!(resp).is_err().matches(|e| {
            matches!(
                e,
                AutocompounderError::CoinNotInPool {
                    denom: _wrong_denom
                }
            )
        });
        Ok(())
    }

    #[test]
    fn cannot_withdraw_liquidity_if_no_claims() -> anyhow::Result<()> {
        let mut deps = app_init(true);
        let msg = AutocompounderExecuteMsg::Withdraw {};
        let resp = execute_as_manager(deps.as_mut(), msg);
        assert_that!(resp)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::NoClaims {}));
        Ok(())
    }

    #[test]
    fn test_check_unbonding_cooldown_with_no_latest_unbonding() {
        let mut deps = mock_dependencies();

        let config = min_cooldown_config(Some(Duration::Time(60)));
        let env = mock_env();
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_unbonding_cooldown_with_expired_unbonding() {
        let mut deps = mock_dependencies();
        let config = min_cooldown_config(Some(Duration::Time(60)));
        let env = mock_env();
        let latest_unbonding = Expiration::AtTime(env.block.time.minus_seconds(60));

        // Exactly expired
        LATEST_UNBONDING
            .save(deps.as_mut().storage, &latest_unbonding)
            .unwrap();
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        assert!(result.is_ok());

        // Expired by 1 second
        let latest_unbonding = Expiration::AtTime(env.block.time.minus_seconds(61));
        LATEST_UNBONDING
            .save(deps.as_mut().storage, &latest_unbonding)
            .unwrap();
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_unbonding_cooldown_with_unexpired_unbonding() {
        let mut deps = mock_dependencies();
        let config = min_cooldown_config(Some(Duration::Time(60)));
        let env = mock_env();

        let latest_unbonding = Expiration::AtTime(env.block.time.minus_seconds(59));
        LATEST_UNBONDING
            .save(deps.as_mut().storage, &latest_unbonding)
            .unwrap();

        // Exactly 1 second left
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        match result {
            Err(AutocompounderError::UnbondingCooldownNotExpired {
                min_cooldown,
                latest_unbonding: latest_unbonding_error,
            }) => {
                assert_eq!(min_cooldown, Duration::Time(60));
                assert_eq!(latest_unbonding, latest_unbonding_error);
            }
            _ => panic!("Unexpected error: {:?}", result),
        }

        // 60 seconds left
        let latest_unbonding = Expiration::AtTime(env.block.time);
        LATEST_UNBONDING
            .save(deps.as_mut().storage, &latest_unbonding)
            .unwrap();
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        match result {
            Err(AutocompounderError::UnbondingCooldownNotExpired {
                min_cooldown,
                latest_unbonding: latest_unbonding_error,
            }) => {
                assert_eq!(min_cooldown, Duration::Time(60));
                assert_eq!(latest_unbonding, latest_unbonding_error);
            }
            _ => panic!("Unexpected error: {:?}", result),
        }
    }

    mod deduct_fee {
        use super::*;

        #[test]
        fn test_deduct_fee_zero() {
            let lp_asset = AnsAsset::new("LP Token".to_string(), Uint128::new(100));
            let fee = Decimal::zero();
            let (lp_asset, fee_asset) = deduct_fee(lp_asset, fee);
            assert_eq!(lp_asset.amount, Uint128::new(100));
            assert_eq!(fee_asset.amount, Uint128::zero());
        }

        #[test]
        fn test_deduct_fee_one() {
            let lp_asset = AnsAsset::new("LP Token".to_string(), Uint128::new(100));
            let fee = Decimal::percent(10);
            let (lp_asset, fee_asset) = deduct_fee(lp_asset, fee);
            assert_eq!(lp_asset.amount, Uint128::new(90));
            assert_eq!(fee_asset.amount, Uint128::new(10));
        }

        #[test]
        fn test_deduct_fee_many() {
            let lp_asset = AnsAsset::new("LP Token".to_string(), Uint128::new(100));
            let fee = Decimal::percent(50);
            let (lp_asset, fee_asset) = deduct_fee(lp_asset, fee);
            assert_eq!(lp_asset.amount, Uint128::new(50));
            assert_eq!(fee_asset.amount, Uint128::new(50));
        }
    }

    mod redeem {
        use super::*;

        #[test]
        fn test_register_pre_claim() {
            let mut deps = mock_dependencies();

            let sender = String::from("sender");
            let sender_addr = Addr::unchecked(sender.clone());
            let amount_of_vault_tokens_to_be_burned = Uint128::new(100);

            // Test case when there is no pending claim for the sender
            let res = register_pre_claim(
                deps.as_mut(),
                sender_addr.clone(),
                amount_of_vault_tokens_to_be_burned,
            );
            assert!(res.is_ok());

            let pending_claim = PENDING_CLAIMS
                .load(deps.as_ref().storage, sender.clone())
                .unwrap();
            assert_eq!(pending_claim, amount_of_vault_tokens_to_be_burned);

            // Test case when there is a pending claim for the sender
            let amount_of_vault_tokens_to_be_burned_2 = Uint128::new(200);
            let res = register_pre_claim(
                deps.as_mut(),
                sender_addr.clone(),
                amount_of_vault_tokens_to_be_burned_2,
            );
            assert!(res.is_ok());

            let pending_claim = PENDING_CLAIMS
                .load(deps.as_ref().storage, sender.clone())
                .unwrap();
            assert_eq!(
                pending_claim,
                amount_of_vault_tokens_to_be_burned + amount_of_vault_tokens_to_be_burned_2
            );
        }
    }
}
