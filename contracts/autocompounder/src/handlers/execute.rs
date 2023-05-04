use super::convert_to_shares;
use super::helpers::{
    check_fee, convert_to_assets, cw20_total_supply, mint_vault_tokens, query_stake,
    stake_lp_tokens,
};
use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_COMPOUND_REPLY_ID, LP_PROVISION_REPLY_ID,
    LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;
use crate::msg::{AutocompounderExecuteMsg, Cw20HookMsg};
use crate::state::{
    Claim, Config, CACHED_ASSETS, CACHED_USER_ADDR, CLAIMS, CONFIG, DEFAULT_BATCH_SIZE, FEE_CONFIG,
    LATEST_UNBONDING, MAX_BATCH_SIZE, PENDING_CLAIMS,
};
use abstract_cw_staking_api::msg::{CwStakingAction, CwStakingExecuteMsg};
use abstract_cw_staking_api::CW_STAKING;
use abstract_dex_api::api::DexInterface;
use abstract_sdk::ApiInterface;
use abstract_sdk::{
    core::objects::{AnsAsset, AssetEntry, LpToken},
    features::{AbstractNameService, AccountIdentification},
    Resolve, TransferInterface,
};
use abstract_sdk::{features::AbstractResponse, AbstractSdkError};
use cosmwasm_std::{
    from_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order,
    ReplyOn, Response, StdResult, SubMsg, Uint128,
};
use cw20::Cw20ReceiveMsg;
use cw_asset::{AssetList, AssetInfoBase, AssetBase};
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
        } => update_fee_config(deps, info, app, performance, withdrawal, deposit),
        AutocompounderExecuteMsg::Deposit { funds } => deposit(deps, info, env, app, funds),
        AutocompounderExecuteMsg::Withdraw {} => withdraw_claims(deps, app, env, info.sender),
        AutocompounderExecuteMsg::BatchUnbond { start_after, limit } => {
            batch_unbond(deps, env, app, start_after, limit)
        }
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

    FEE_CONFIG.save(deps.storage, &config)?;

    Ok(app.custom_tag_response(Response::new(), "update_fee_config", updates))
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

    // TODO: this resolution is probably not necessary as we should store the asset addressses for the configured pool
    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits
        .deduct_many(&msg_info.funds.clone().into())?
        .purge();

    // if there is only one asset, we need to add the other asset too, but with zero amount
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

    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>, AbstractSdkError> = claimed_deposits
        .into_iter()
        .map(|asset| {
            // transfer cw20 tokens to the Account
            // will fail if allowance is not set or if some other assets are sent
            Ok(asset.transfer_from_msg(&msg_info.sender, app.proxy_address(deps.as_ref())?)?)
        })
        .collect();
    msgs.append(cw_20_transfer_msgs_res?.as_mut());

    // transfer received coins to the Account
    if !msg_info.funds.is_empty() {
        let bank = app.bank(deps.as_ref());
        msgs.push(bank.deposit_coins(msg_info.funds)?);
    }

    let dex = app.dex(deps.as_ref(), config.pool_data.dex);
    let provide_liquidity_msg: CosmosMsg = dex.provide_liquidity(
        funds,
        // TODO: let the user provide this
        Some(Decimal::percent(5)),
    )?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: provide_liquidity_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &msg_info.sender)?;
    let response = Response::new().add_messages(msgs).add_submessage(sub_msg);
    Ok(app.custom_tag_response(response, "deposit", vec![("4t2", "/AC/Deposit")]))
}

pub fn batch_unbond(
    deps: DepsMut,
    env: Env,
    app: AutocompounderApp,
    start_after: Option<String>,
    limit: Option<u32>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    if config.unbonding_period.is_none() {
        return Err(AutocompounderError::UnbondingNotEnabled {});
    }

    // check if the cooldown period has passed
    check_unbonding_cooldown(&deps, &config, &env)?;

    let limit = limit.unwrap_or(DEFAULT_BATCH_SIZE).min(MAX_BATCH_SIZE) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into_bytes()));
    let pending_claims = PENDING_CLAIMS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect::<StdResult<Vec<(String, Uint128)>>>()?;

    let (total_lp_amount_to_unbond, total_vault_tokens_to_burn, updated_claims) =
        calculate_withdrawals(deps.as_ref(), &config, &app, pending_claims.clone(), env)?;

    // clear pending claims
    for claim in pending_claims.iter() {
        PENDING_CLAIMS.remove(deps.storage, claim.0.clone());
    }
    // PENDING_CLAIMS.clear(deps.storage);
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
    // Withdraw fn can only be called by liquidity token or the lp token
    match from_binary(&msg.msg)? {
        Cw20HookMsg::Redeem {} => redeem(deps, env, app, info.sender, msg.sender, msg.amount),
        Cw20HookMsg::DepositLp {} => {
            deposit_lp(deps, env, app, info.sender, msg.sender, msg.amount)
        }
    }
}

fn deposit_lp(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    cw20_sender: Addr,
    sender: String,
    amount: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let ans_host = app.ans_host(deps.as_ref())?;
    let mut submessages = vec![];
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());
    if cw20_sender != config.liquidity_token {
        return Err(AutocompounderError::SenderIsNotLpToken {});
    };
    let lp_token = LpToken::from(config.pool_data.clone());
    let transfer_msgs = app.bank(deps.as_ref()).deposit(vec![
            AnsAsset::new(AssetEntry::from(lp_token.clone()),amount)
        ])?;

    let sender = deps.api.addr_validate(&sender)?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_token.clone().into(),
        config.unbonding_period,
    )?;
    let current_vault_supply = cw20_total_supply(deps.as_ref(), &config)?;

    let assigned_amount = if !fee_config.deposit.is_zero() {
        let fee = amount * fee_config.deposit;
        let withdraw_msg = dex.withdraw_liquidity(lp_token.clone().into(), amount)?;
        let withdraw_sub_msg = SubMsg {
            id: LP_WITHDRAWAL_REPLY_ID,
            msg: withdraw_msg,
            gas_limit: None,
            reply_on: ReplyOn::Success,
        };
        submessages.push(withdraw_sub_msg);

        // save cached assets
        let owned_assets = owned_assets(config.pool_data.assets.clone(), &deps, ans_host, &app)?;
        for (owned_asset, owned_amount) in owned_assets {
            CACHED_ASSETS.save(deps.storage, owned_asset, &owned_amount)?;
        }

        amount - fee
    } else {
        amount
    };

    let mint_amount = convert_to_shares(assigned_amount, staked_lp, current_vault_supply, 0);
    let mint_msg = mint_vault_tokens(&config, sender, mint_amount)?;
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, amount),
        config.unbonding_period,
    )?;

    Ok(app.custom_tag_response(
        Response::new()
            .add_messages(transfer_msgs)
            .add_messages(vec![mint_msg, stake_msg])
            .add_submessages(submessages),
        "deposit-lp",
        vec![("4t2", "/AC/DepositLP")],
    ))
}

fn redeem(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    cw20_sender: Addr,
    sender: String,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    if cw20_sender != config.vault_token {
        return Err(AutocompounderError::SenderIsNotVaultToken {});
    }

    // parse sender
    let sender = deps.api.addr_validate(&sender)?;

    // save the user address to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, &sender)?;

    if config.unbonding_period.is_none() {
        // if bonding period is not set, we can just burn the tokens, and withdraw the underlying assets in the lp pool.
        // 1) get the total supply of Vault token
        let total_supply_vault = cw20_total_supply(deps.as_ref(), &config)?;
        let lp_token = LpToken::from(config.pool_data.clone());

        // 2) get total staked lp token
        let total_lp_tokens_staked_in_vault = query_stake(
            deps.as_ref(),
            &app,
            config.pool_data.dex.clone(),
            lp_token.into(),
            None,
        )?;

        let lp_tokens_withdraw_amount = convert_to_assets(
            amount_of_vault_tokens_to_be_burned,
            total_lp_tokens_staked_in_vault,
            total_supply_vault,
            0,
        );

        // unstake lp tokens
        let unstake_msg = unstake_lp_tokens(
            deps.as_ref(),
            &app,
            config.pool_data.dex.clone(),
            AssetEntry::from(LpToken::from(config.pool_data.clone())),
            lp_tokens_withdraw_amount,
            None,
        );
        let burn_msg = get_burn_msg(&config.vault_token, amount_of_vault_tokens_to_be_burned)?;

        // 3) withdraw lp tokens
        let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());
        let withdraw_msg: CosmosMsg = dex.withdraw_liquidity(
            LpToken::from(config.pool_data).into(),
            lp_tokens_withdraw_amount,
        )?;
        let sub_msg = SubMsg::reply_on_success(withdraw_msg, LP_WITHDRAWAL_REPLY_ID);

        let response = Response::new()
            .add_message(unstake_msg)
            .add_message(burn_msg)
            .add_submessage(sub_msg);
        Ok(app.custom_tag_response(response, "redeem", vec![("4t2", "AC/Redeem")]))
    } else {
        // if bonding period is set, we need to register the user's pending claim, that will be processed in the next batch unbonding
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

        Ok(app.custom_tag_response(
            Response::new(),
            "redeem",
            vec![("4t2", "AC/Register_pre_claim")],
        ))
    }
}

fn compound(deps: DepsMut, app: AutocompounderApp) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // 1) Claim rewards from staking contract
    let claim_msg = claim_lp_rewards(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
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

    let response = Response::new().add_submessage(claim_submsg);
    Ok(app.tag_response(response, "compound"))
}

/// withdraw all matured claims for a user
pub fn withdraw_claims(
    deps: DepsMut,
    app: AutocompounderApp,
    env: Env,
    address: Addr,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let pool_assets = config.pool_data.assets.clone();
    let ans_host = app.ans_host(deps.as_ref())?;

    let owned_assets = owned_assets(pool_assets, &deps, ans_host, &app)?;
    owned_assets.into_iter().for_each(|(asset, amount)| {
        CACHED_ASSETS.save(deps.storage, asset, &amount).unwrap();
    });
    CACHED_USER_ADDR.save(deps.storage, &address)?;

    if config.unbonding_period.is_none() {
        return Err(AutocompounderError::UnbondingNotEnabled {});
    }

    let Some(claims) = CLAIMS.may_load(deps.storage, address.to_string())? else {
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
        eprintln!(
            "No matured claims at timestamp: {:?}. ongoing claims: {ongoing_claims:?}",
            env.block.time
        );
        return Err(AutocompounderError::NoMaturedClaims {});
    }

    CLAIMS.save(deps.storage, address.to_string(), &ongoing_claims)?;

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
        AssetEntry::from(LpToken::from(config.pool_data.clone())),
    );

    // 3) withdraw lp tokens
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());
    let swap_msg: CosmosMsg = dex.withdraw_liquidity(
        LpToken::from(config.pool_data).into(),
        lp_tokens_to_withdraw,
    )?;
    let sub_msg = SubMsg::reply_on_success(swap_msg, LP_WITHDRAWAL_REPLY_ID);

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

fn owned_assets(
    for_assets: Vec<AssetEntry>,
    deps: &DepsMut,
    ans_host: abstract_sdk::feature_objects::AnsHost,
    app: &abstract_app::AppContract<
        AutocompounderError,
        crate::msg::AutocompounderInstantiateMsg,
        AutocompounderExecuteMsg,
        crate::msg::AutocompounderQueryMsg,
        crate::msg::AutocompounderMigrateMsg,
        Cw20ReceiveMsg,
    >,
) -> Result<Vec<(String, Uint128)>, AutocompounderError> {
    let owned_assets = for_assets
        .into_iter()
        .map(|asset| {
            let asset_info = asset.resolve(&deps.querier, &ans_host)?;
            let amount = asset_info
                .query_balance(&deps.querier, app.proxy_address(deps.as_ref())?.to_string())?;
            Ok((asset.to_string(), amount))
        })
        .collect::<Result<Vec<(String, Uint128)>, AutocompounderError>>()?;
    Ok(owned_assets)
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
        .unbonding_period
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

        let user_lp_tokens_withdraw_amount = convert_to_assets(user_amount_of_vault_tokens_to_be_burned, 
            total_lp_tokens_staked_in_vault, vault_tokens_total_supply, 0);

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

fn claim_lp_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let apis = app.apis(deps);

    apis.request(
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
fn claim_unbonded_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
) -> CosmosMsg {
    let apis = app.apis(deps);

    apis.request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Claim {
                staking_token: lp_token_name,
            },
        },
    )
    .unwrap()
}

fn get_burn_msg(contract: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::Burn { amount };

    Ok(wasm_execute(contract.to_string(), &msg, vec![])?.into())
}

fn unstake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
    unbonding_period: Option<Duration>,
) -> CosmosMsg {
    let apis = app.apis(deps);

    apis.request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Unstake {
                staking_token: AnsAsset::new(lp_token_name, amount),
                unbonding_period,
            },
        },
    )
    .unwrap()
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::msg::ExecuteMsg;
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_init};
    use abstract_core::objects::pool_id::PoolAddressBase;
    use abstract_core::objects::PoolMetadata;
    use abstract_sdk::base::ExecuteEndpoint;
    use abstract_testing::prelude::TEST_MANAGER;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::Storage;
    use cw_controllers::AdminError;
    use cw_utils::Expiration;
    use speculoos::{assert_that, result::ResultAssertions};

    fn execute_as(
        deps: DepsMut,
        sender: &str,
        msg: impl Into<ExecuteMsg>,
    ) -> Result<Response, AutocompounderError> {
        let info = mock_info(sender, &[]);
        AUTOCOMPOUNDER_APP.execute(deps, mock_env(), info, msg.into())
    }

    fn execute_as_manager(
        deps: DepsMut,
        msg: impl Into<ExecuteMsg>,
    ) -> Result<Response, AutocompounderError> {
        execute_as(deps, TEST_MANAGER, msg)
    }

    fn min_cooldown_config(min_unbonding_cooldown: Option<Duration>) -> Config {
        let assets = vec![AssetEntry::new("juno>juno")];

        Config {
            staking_contract: Addr::unchecked("staking_contract"),
            pool_address: PoolAddressBase::Contract(Addr::unchecked("pool_address")),
            pool_data: PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                assets,
            ),
            pool_assets: vec![],
            liquidity_token: Addr::unchecked("liquidity_token"),
            vault_token: Addr::unchecked("vault_token"),
            unbonding_period: Some(Duration::Time(100)),
            min_unbonding_cooldown,
        }
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
            };

            let resp = execute_as(deps.as_mut(), "not_mananger", msg.clone());
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
            };

            let resp = execute_as_manager(deps.as_mut(), msg);
            assert_that!(resp)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::InvalidFee {}));
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
}
