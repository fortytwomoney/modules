use super::convert_to_shares;

use super::helpers::{
    burn_vault_tokens_msg, check_fee, convert_to_assets, get_unbonding_period_and_cooldown,
    mint_vault_tokens_msg, query_stake, stake_lp_tokens, transfer_to_msgs,
    vault_token_total_supply,
};

use abstract_core::objects::AnsEntryConvertor;
use abstract_sdk::feature_objects::AnsHost;
use abstract_sdk::{AccountAction, AdapterInterface};

use crate::contract::{
    AutocompounderApp, AutocompounderResult, LP_COMPOUND_REPLY_ID, LP_PROVISION_REPLY_ID,
    LP_WITHDRAWAL_REPLY_ID,
};
use crate::error::AutocompounderError;

use crate::msg::{AutocompounderExecuteMsg, BondingData};
use crate::state::{
    Claim, Config, FeeConfig, CACHED_ASSETS, CACHED_USER_ADDR, CLAIMS, CONFIG, DEFAULT_BATCH_SIZE,
    FEE_CONFIG, LATEST_UNBONDING, MAX_BATCH_SIZE, PENDING_CLAIMS,
};
use abstract_cw_staking::msg::{StakingAction, StakingExecuteMsg};
use abstract_cw_staking::CW_STAKING_ADAPTER_ID;
use abstract_dex_adapter::api::DexInterface;
use abstract_sdk::Execution;
use abstract_sdk::{
    core::objects::{AnsAsset, AssetEntry},
    features::{AbstractNameService, AccountIdentification},
    Resolve, TransferInterface,
};
use abstract_sdk::{features::AbstractResponse, AbstractSdkError};
use cosmwasm_std::{
    Addr, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Order, ReplyOn, Response,
    StdError, StdResult, SubMsg, Uint128,
};
use cw20::Cw20ReceiveMsg;
use cw_asset::{Asset, AssetBase, AssetInfoBase, AssetList};
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
    // if the msg is not of variant 'CreateDenom' then check if the vault token is initialized
    // pre_execute_check(&msg, deps.as_ref())?;

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
        AutocompounderExecuteMsg::Deposit {
            funds,
            recipient,
            max_spread,
        } => deposit(deps, info, env, app, funds, recipient, max_spread),
        AutocompounderExecuteMsg::DepositLp {
            lp_token,
            recipient: receiver,
        } => deposit_lp(deps, info, env, app, lp_token, receiver),
        AutocompounderExecuteMsg::Redeem { amount, recipient } => {
            redeem(deps, env, app, info, amount, recipient)
        }
        AutocompounderExecuteMsg::Withdraw {} => withdraw_claims(deps, app, env, info.sender),
        AutocompounderExecuteMsg::BatchUnbond { start_after, limit } => {
            batch_unbond(deps, env, app, start_after, limit)
        }
        AutocompounderExecuteMsg::Compound {} => compound(deps, app),
        AutocompounderExecuteMsg::UpdateStakingConfig { bonding_data } => {
            update_staking_config(deps, app, info, bonding_data)
        }
    }
}

pub fn update_staking_config(
    deps: DepsMut,
    app: AutocompounderApp,
    info: MessageInfo,
    bonding_data: Option<BondingData>,
) -> AutocompounderResult {
    app.admin.assert_admin(deps.as_ref(), &info.sender)?;

    let mut config = CONFIG.load(deps.storage)?;

    let (new_unbonding_period, new_min_unbonding_cooldown) =
        get_unbonding_period_and_cooldown(bonding_data)?;

    config.unbonding_period = new_unbonding_period;
    config.min_unbonding_cooldown = new_min_unbonding_cooldown;
    CONFIG.save(deps.storage, &config)?;

    Ok(app.custom_response(
        "update_config_with_staking_contract_data",
        vec![(
            "unbonding_period",
            config
                .unbonding_period
                .map_or("none".to_string(), |f| format!("{:?}", f)),
        )],
    ))
}

/// Update the application configuration.
pub fn update_fee_config(
    deps: DepsMut,
    info: MessageInfo,
    app: AutocompounderApp,
    fee: Option<Decimal>,
    withdrawal: Option<Decimal>,
    deposit: Option<Decimal>,
    fee_collector_addr: Option<String>,
) -> AutocompounderResult {
    app.admin.assert_admin(deps.as_ref(), &info.sender)?;

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

    Ok(app.custom_response("update_fee_config", updates))
}

// This is the function that is called when the user wants to pool AND stake their funds
pub fn deposit(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    app: AutocompounderApp,
    mut funds: Vec<AnsAsset>,
    recipient: Option<Addr>,
    max_spread: Option<Decimal>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;

    let ans_host = app.ans_host(deps.as_ref())?;
    let dex = app.ans_dex(deps.as_ref(), config.pool_data.dex.clone());

    let mut messages = vec![];
    let mut submessages = vec![];

    // consolidate ans assets with funds

    let info_ans_assets = resolve_info_funds(info.funds.clone(), &deps.as_ref(), &ans_host)?;
    consolidate_funds(&mut funds, info_ans_assets.clone())?;

    // check if all the assets in funds are present in the pool
    check_all_funds_in_pool(&funds, &config)?;

    let mut claimed_deposits: AssetList = funds.resolve(&deps.querier, &ans_host)?.into();
    // deduct all the received `Coin`s from the claimed deposit, errors if not enough funds were provided
    // what's left should be the remaining cw20s
    claimed_deposits
        .deduct_many(&info.funds.clone().into())?
        .purge();

    // if there is only one asset, we need to add the other asset too, but with zero amount
    let cw_20_transfer_msgs_res: Result<Vec<CosmosMsg>, AbstractSdkError> = claimed_deposits
        .into_iter()
        .map(|asset| {
            // transfer cw20 tokens to the Account
            // will fail if allowance is not set or if some other assets are sent
            Ok(asset.transfer_from_msg(&info.sender, app.proxy_address(deps.as_ref())?)?)
        })
        .collect();
    messages.append(cw_20_transfer_msgs_res?.as_mut());

    let mut account_msgs = AccountAction::new();
    // transfer received coins to the Account
    if !info.funds.is_empty() {
        let bank = app.bank(deps.as_ref());
        messages.extend(bank.deposit(info.funds)?);
    }

    // deduct deposit fee
    if !fee_config.deposit.is_zero() {
        let fees = deduct_deposit_fees(&mut funds, &fee_config);

        // 3) Send fees to the feecollector
        if !fees.is_empty() {
            let transfer_msg = app
                .bank(deps.as_ref())
                .transfer(fees, &fee_config.fee_collector_addr)?;
            account_msgs.merge(transfer_msg);
        }
    }

    add_asset_if_single_fund(&mut funds, &config);

    let provide_liquidity_msg: CosmosMsg =
        dex.provide_liquidity(funds, Some(max_spread.unwrap_or(config.max_swap_spread)))?;

    let sub_msg = SubMsg {
        id: LP_PROVISION_REPLY_ID,
        msg: provide_liquidity_msg,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };
    submessages.push(sub_msg);

    // save the user address to the cache for later use in reply

    // CACHED_FEE_AMOUNT.save(deps.storage, &current_fee_balance)?;
    let recipient = unwrap_recipient_is_allowed(
        recipient,
        &info.sender,
        forbidden_deposit_addresses(deps.as_ref(), &env, &app)?,
    )?;
    CACHED_USER_ADDR.save(deps.storage, &recipient)?;

    let mut response = app
        .custom_response("deposit", vec![("recipient", recipient.to_string())])
        .add_messages(messages)
        .add_submessages(submessages);

    if !account_msgs.messages().is_empty() {
        response = response.add_message(app.executor(deps.as_ref()).execute(vec![account_msgs])?);
    }

    Ok(response)
}

fn consolidate_funds(
    funds: &mut [AnsAsset],
    info_ans_assets: Vec<AnsAsset>,
) -> AutocompounderResult<()> {
    info_ans_assets
        .iter()
        .try_for_each(|info_asset: &AnsAsset| -> AutocompounderResult<()> {
            if let Some(fund) = funds.iter_mut().find(|f| f.name.eq(&info_asset.name)) {
                if fund.amount.u128() == 0u128 {
                    if info_asset.amount.u128() > 0u128 {
                        fund.amount = info_asset.amount;
                    } // else both zero, so do nothing
                } else if fund.amount != info_asset.amount {
                    return Err(AutocompounderError::FundsMismatch {
                        wanted_funds: funds.iter().map(|a| a.to_string()).collect(),
                        sent_funds: info_ans_assets.iter().map(|a| a.to_string()).collect(),
                    });
                }
            } else {
                return Err(AutocompounderError::FundsMismatch {
                    wanted_funds: funds.iter().map(|a| a.to_string()).collect(),
                    sent_funds: info_ans_assets.iter().map(|a| a.to_string()).collect(),
                });
            }
            Ok(())
        })
}

fn resolve_info_funds(
    info_funds: Vec<Coin>,
    deps: &Deps<'_>,
    ans_host: &AnsHost,
) -> Result<Vec<AnsAsset>, AutocompounderError> {
    let info_assets = info_funds
        .into_iter()
        .map(|f| Asset::native(f.denom, f.amount))
        .collect::<Vec<Asset>>();
    let info_ans_assets = info_assets.resolve(&deps.querier, ans_host)?;
    Ok(info_ans_assets)
}

/// Add the other asset if there is only one asset. This is needed for the lp provision to work.
fn add_asset_if_single_fund(funds: &mut Vec<AnsAsset>, config: &Config) {
    if funds.len() == 1 {
        config.pool_data.assets.iter().for_each(|asset| {
            if !funds[0].name.eq(asset) {
                funds.push(AnsAsset::new(asset.clone(), 0u128))
            }
        });
    };
}

fn deduct_deposit_fees(funds: &mut [AnsAsset], fee_config: &FeeConfig) -> Vec<AnsAsset> {
    let mut fees = vec![];
    funds.iter_mut().for_each(|asset| {
        let fee = asset.amount * fee_config.deposit;
        let fee_asset = AnsAsset::new(asset.name.clone(), fee);
        asset.amount -= fee;
        if !fee.is_zero() {
            fees.push(fee_asset);
        }
    });
    fees
}

fn check_all_funds_in_pool(funds: &[AnsAsset], config: &Config) -> Result<(), AutocompounderError> {
    for asset in funds.iter() {
        if !config.pool_data.assets.contains(&asset.name) {
            return Err(AutocompounderError::AssetNotInPool {
                asset: asset.name.to_string(),
            });
        }
    }
    Ok(())
}

fn unwrap_recipient_is_allowed(
    recipient: Option<Addr>,
    sender: &Addr,
    disallowed: Vec<Addr>,
) -> Result<Addr, AutocompounderError> {
    if let Some(recipient) = recipient {
        if disallowed.contains(&recipient) {
            return Err(AutocompounderError::CannotSetRecipientToAccount {});
        }
        Ok(recipient)
    } else {
        Ok(sender.clone())
    }
}

fn forbidden_deposit_addresses(
    deps: Deps,
    env: &Env,
    app: &AutocompounderApp,
) -> Result<Vec<Addr>, AutocompounderError> {
    Ok(vec![app.proxy_address(deps)?, env.contract.address.clone()])
}

fn deposit_lp(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    app: AutocompounderApp,
    lp_asset: AnsAsset,
    recipient: Option<Addr>,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let ans = app.name_service(deps.as_ref());
    let lp_token = ans.query(&lp_asset)?;
    let lp_asset_entry = lp_asset.name.clone();
    let recipient = unwrap_recipient_is_allowed(
        recipient,
        &info.sender,
        forbidden_deposit_addresses(deps.as_ref(), &env, &app)?,
    )?;

    if lp_token.info != config.liquidity_token {
        return Err(AutocompounderError::SenderIsNotLpToken {});
    };

    // transfer the asset to the proxy contract
    let transfer_msg = transfer_token_to_proxy(lp_token, info.sender, &app, deps.as_ref())?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_asset_entry,
        config.unbonding_period,
    )?;

    let (lp_asset, fee_asset) = deduct_fee(lp_asset, fee_config.deposit);
    let fee_msg = transfer_to_msgs(
        &app,
        deps.as_ref(),
        fee_asset,
        &fee_config.fee_collector_addr,
    )?;

    let current_vault_supply = vault_token_total_supply(deps.as_ref(), &config)?;
    let mint_amount = convert_to_shares(lp_asset.amount, staked_lp, current_vault_supply);
    if mint_amount.is_zero() {
        return Err(AutocompounderError::ZeroMintAmount {});
    }

    let mint_msg = mint_vault_tokens_msg(
        &config,
        &env.contract.address,
        recipient.clone(),
        mint_amount,
        config.pool_data.dex.clone(),
    )?;
    let stake_msg = stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        lp_asset,
        config.unbonding_period,
    )?;

    Ok(app
        .custom_response("deposit-lp", vec![("recipient", recipient.to_string())])
        .add_message(transfer_msg)
        .add_messages(vec![mint_msg, stake_msg])
        .add_message(fee_msg))
}

/// Deducts a specified fee from a given LP asset.
///
/// If the fee is zero, it returns the original LP asset and a fee asset with zero amount.
/// Otherwise, it calculates the fee amount, deducts it from the LP asset, and assigns it to the fee asset.
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
fn transfer_token_to_proxy(
    lp_token: Asset,
    sender: Addr,
    app: &AutocompounderApp,
    deps: Deps,
) -> Result<CosmosMsg, AutocompounderError> {
    match lp_token.info.clone() {
        AssetInfoBase::Cw20(_addr) => Asset::cw20(_addr, lp_token.amount)
            .transfer_from_msg(sender, app.proxy_address(deps)?)
            .map_err(|e| e.into()),
        AssetInfoBase::Native(_denom) => Ok(app
            .bank(deps)
            .deposit(vec![lp_token])?
            .into_iter()
            .next()
            .ok_or(AutocompounderError::Std(StdError::generic_err(
                "no message",
            )))?),
        _ => Err(AutocompounderError::AssetError(
            cw_asset::AssetError::InvalidAssetFormat {
                received: lp_token.to_string(),
            },
        )),
    }
}

fn transfer_token_to_autocompounder(
    asset: Asset,
    sender: Addr,
    env: &Env,
    funds: &[Coin],
) -> Result<Vec<CosmosMsg>, AutocompounderError> {
    match asset.info.clone() {
        AssetInfoBase::Cw20(addr) => {
            if !funds.is_empty() {
                return Err(AutocompounderError::FundsMismatch {
                    wanted_funds: asset.to_string(),
                    sent_funds: funds[0].to_string(),
                });
            }
            Ok(vec![Asset::cw20(addr, asset.amount)
                .transfer_from_msg(sender, env.contract.address.clone())
                .map_err(|e| -> AutocompounderError { e.into() })?])
        }
        AssetInfoBase::Native(denom) => {
            let required_funds = vec![Coin {
                denom,
                amount: asset.amount,
            }];
            if funds != required_funds {
                return Err(AutocompounderError::FundsMismatch {
                    wanted_funds: required_funds[0].to_string(),
                    sent_funds: funds[0].to_string(),
                });
            }

            Ok(vec![])
        }
        _ => Err(AutocompounderError::AssetError(
            cw_asset::AssetError::InvalidAssetFormat {
                received: asset.to_string(),
            },
        )),
    }
}

/// Unbonds a batch of tokens from the Autocompounder.
///
/// This function handles the unbonding process for a batch of tokens. It first checks if the unbonding
/// period is set in the configuration. If not, it returns an error indicating that unbonding is not enabled.
///
/// If the unbonding period is set, the function checks if the cooldown period for unbonding has passed.
/// It then determines the number of claims to process based on the provided limit or the default batch size.
/// The function fetches the pending claims and calculates the total amount of LP tokens to unbond and the
/// total number of vault tokens to burn.
///
/// After calculating the withdrawals, the function clears the processed pending claims and updates the claims.
/// It then constructs messages to unstake the LP tokens and burn the vault tokens.
///
/// Finally, the function returns a response with the constructed messages and a custom tag indicating that
/// the batch unbonding process was executed.
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
        .collect::<StdResult<Vec<(Addr, Uint128)>>>()?;

    let (total_lp_amount_to_unbond, total_vault_tokens_to_burn, updated_claims) =
        calculate_withdrawals(
            deps.as_ref(),
            &config,
            &fee_config,
            &app,
            pending_claims.clone(),
            &env,
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
        AnsEntryConvertor::new(AnsEntryConvertor::new(config.pool_data.clone()).lp_token())
            .asset_entry(),
        total_lp_amount_to_unbond,
        config.unbonding_period,
    );

    let burn_msg = burn_vault_tokens_msg(
        &config,
        &env.contract.address,
        total_vault_tokens_to_burn,
        config.pool_data.dex.clone(),
    )?;

    Ok(app
        .custom_response(
            "batch_unbond",
            vec![
                ("unbond_amount", total_lp_amount_to_unbond.to_string()),
                ("burn_amount", total_vault_tokens_to_burn.to_string()),
            ],
        )
        .add_messages(vec![unstake_msg, burn_msg]))
}

/// Handles receiving CW20 messages
pub fn receive(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _app: AutocompounderApp,
    _msg: Cw20ReceiveMsg,
) -> AutocompounderResult {
    Err(AutocompounderError::Std(
        cosmwasm_std::StdError::GenericErr {
            msg: "cannot recieve c20 tokens. Deposit and redeem using allowance".to_string(),
        },
    ))
}

/// Redeems the vault tokens for the underlying asset.
/// This function is called by the vault token contract.
/// It checks whether the lp staking contract has a unbonding period set or not.
/// If not, it redeems the vault tokens for the underlying asset, swaps them and sends them to the sender.
/// If yes, it registers a pre-claim for the sender.  This will be processed in batches by calling `ExecuteMsg::BatchUnbond` .
fn redeem(
    deps: DepsMut,
    env: Env,
    app: AutocompounderApp,
    info: MessageInfo,
    amount_of_vault_tokens_to_be_burned: Uint128,
    recipient: Option<Addr>,
) -> AutocompounderResult {
    // parse sender
    let recipient = unwrap_recipient_is_allowed(
        recipient,
        &info.sender,
        forbidden_deposit_addresses(deps.as_ref(), &env, &app)?,
    )?;
    let config = CONFIG.load(deps.storage)?;

    if config.unbonding_period.is_none() {
        redeem_without_bonding_period(
            deps,
            &env,
            &recipient,
            info,
            config,
            &app,
            amount_of_vault_tokens_to_be_burned,
        )
    } else {
        receive_and_register_claim(
            deps,
            &env,
            app,
            recipient,
            info,
            amount_of_vault_tokens_to_be_burned,
        )
    }
}

/// Registers a pending-claim when Redeem is called for a pool with bonding period.
/// This will store the claim of te user and add it to any pending claims.
/// The claim will be processed in the next batch unbonding
fn register_pre_claim(
    deps: DepsMut,
    for_address: Addr,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> Result<(), AutocompounderError> {
    // if bonding period is set, we need to register the user's pending claim, that will be processed in the next batch unbonding
    if let Some(pending_claim) = PENDING_CLAIMS.may_load(deps.storage, for_address.clone())? {
        let new_pending_claim = pending_claim
            .checked_add(amount_of_vault_tokens_to_be_burned)
            .unwrap();
        PENDING_CLAIMS.save(deps.storage, for_address, &new_pending_claim)?;
    // if not, we just store a new claim
    } else {
        PENDING_CLAIMS.save(
            deps.storage,
            for_address,
            &amount_of_vault_tokens_to_be_burned,
        )?;
    }

    Ok(())
}

fn receive_and_register_claim(
    deps: DepsMut,
    env: &Env,
    app: AutocompounderApp,
    recipient: Addr,
    info: MessageInfo,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.as_ref().storage)?;
    let sender = info.sender.clone();

    let vault_token = AssetBase::new(config.vault_token, amount_of_vault_tokens_to_be_burned);
    let transfer_msgs = transfer_token_to_autocompounder(vault_token, sender, env, &info.funds)?;

    register_pre_claim(deps, recipient.clone(), amount_of_vault_tokens_to_be_burned)?;

    Ok(app
        .custom_response(
            "claim_registered",
            vec![
                ("recipient", recipient.to_string()),
                ("amount", amount_of_vault_tokens_to_be_burned.to_string()),
            ],
        )
        .add_messages(transfer_msgs))
}

/// Redeems the vault tokens without a bonding period.
/// This will unstake the lp tokens, burn the vault tokens, withdraw the underlying assets and send them to the user
fn redeem_without_bonding_period(
    deps: DepsMut,
    env: &Env,
    recipient: &Addr,
    info: MessageInfo,
    config: Config,
    app: &AutocompounderApp,
    amount_of_vault_tokens_to_be_burned: Uint128,
) -> Result<Response, AutocompounderError> {
    let fee_config = FEE_CONFIG.load(deps.storage)?;
    let sender = info.sender.clone();

    let vault_token_asset = AssetBase::new(
        config.vault_token.clone(),
        amount_of_vault_tokens_to_be_burned,
    );
    let transfer_msgs =
        transfer_token_to_autocompounder(vault_token_asset, sender, env, &info.funds)?;

    // save the user address and the assets owned by the contract to the cache for later use in reply
    CACHED_USER_ADDR.save(deps.storage, recipient)?;
    let owned_assets = app.bank(deps.as_ref()).balances(&config.pool_data.assets)?;
    owned_assets.into_iter().try_for_each(|asset| {
        CACHED_ASSETS
            // CACHED_ASSETS are saved with the key being cwasset::asset:AssetInfo.to_string()
            .save(deps.storage, asset.info.to_string(), &asset.amount)
            .map_err(AutocompounderError::Std)
    })?;

    // 1) get the total supply of Vault token
    let total_supply_vault = vault_token_total_supply(deps.as_ref(), &config)?;
    let lp_asset_entry = config.lp_asset_entry();

    // 2) get total staked lp token
    let total_lp_tokens_staked_in_vault = query_stake(
        deps.as_ref(),
        app,
        config.pool_data.dex.clone(),
        lp_asset_entry.clone(),
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
        lp_asset_entry.clone(),
        lp_tokens_withdraw_amount,
        None,
    );
    let burn_msg = burn_vault_tokens_msg(
        &config,
        &env.contract.address,
        amount_of_vault_tokens_to_be_burned,
        config.pool_data.dex.clone(),
    )?;

    // 3) withdraw lp tokens
    let dex = app.ans_dex(deps.as_ref(), config.pool_data.dex);
    let withdraw_msg: CosmosMsg =
        dex.withdraw_liquidity(AnsAsset::new(lp_asset_entry, lp_tokens_withdraw_amount))?;
    let sub_msg = SubMsg::reply_on_success(withdraw_msg, LP_WITHDRAWAL_REPLY_ID);

    // TODO: Check all the lp_token() calls and make sure they are everywhere.

    Ok(app
        .custom_response(
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
        )
        .add_messages(transfer_msgs)
        .add_message(unstake_msg)
        .add_message(burn_msg)
        .add_submessage(sub_msg))
}

fn compound(deps: DepsMut, app: AutocompounderApp) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;

    // 1) Claim rewards from staking contract
    let claim_msg = claim_lp_rewards(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        config.lp_asset_entry(),
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

    Ok(app.response("compound").add_submessage(claim_submsg))
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

    let Some(claims) = CLAIMS.may_load(deps.storage, sender.clone())? else {
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

    CLAIMS.save(deps.storage, sender.clone(), &ongoing_claims)?;

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
        config.lp_asset_entry(),
    );

    // 3) withdraw lp tokens
    let dex = app.ans_dex(deps.as_ref(), config.pool_data.dex.clone());
    let withdraw_msg: CosmosMsg = dex.withdraw_liquidity(AnsAsset::new(
        config.lp_asset_entry(),
        lp_tokens_to_withdraw,
    ))?;
    let sub_msg = SubMsg::reply_on_success(withdraw_msg, LP_WITHDRAWAL_REPLY_ID);

    Ok(app
        .custom_response(
            "withdraw_claims",
            vec![
                ("recipient", sender.to_string()),
                ("lp_tokens_to_withdraw", lp_tokens_to_withdraw.to_string()),
            ],
        )
        .add_message(claim_msg)
        .add_submessage(sub_msg))
}

#[allow(clippy::type_complexity)]
/// Calculates the amount the total amount of lp tokens to unbond and vault tokens to burn
fn calculate_withdrawals(
    deps: Deps,
    config: &Config,
    fee_config: &FeeConfig,
    app: &AutocompounderApp,
    pending_claims: Vec<(Addr, Uint128)>,
    env: &Env,
) -> Result<(Uint128, Uint128, Vec<(Addr, Vec<Claim>)>), AutocompounderError> {
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
    let vault_tokens_total_supply = vault_token_total_supply(deps, config)?;

    // 2) get total staked lp token
    let total_lp_tokens_staked_in_vault = query_stake(
        deps,
        app,
        config.pool_data.dex.clone(),
        lp_token,
        config.unbonding_period,
    )?;

    let mut updated_claims: Vec<(Addr, Vec<Claim>)> = vec![];
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
            CW_STAKING_ADAPTER_ID,
            StakingExecuteMsg {
                provider,
                action: StakingAction::ClaimRewards {
                    assets: vec![lp_token_name],
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
            CW_STAKING_ADAPTER_ID,
            StakingExecuteMsg {
                provider,
                action: StakingAction::Claim {
                    assets: vec![lp_token_name],
                },
            },
        )
        .unwrap()
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
            CW_STAKING_ADAPTER_ID,
            StakingExecuteMsg {
                provider,
                action: StakingAction::Unstake {
                    assets: vec![AnsAsset::new(lp_token_name, amount)],
                    unbonding_period,
                },
            },
        )
        .unwrap()
}

#[cfg(test)]
mod test {
    use super::{redeem_without_bonding_period, *};

    use crate::handlers::helpers::helpers_tests::min_cooldown_config;
    use crate::msg::ExecuteMsg;
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_init};

    use abstract_sdk::base::ExecuteEndpoint;

    use abstract_testing::prelude::{TEST_MANAGER, TEST_PROXY};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{Attribute, Coin};
    use cw_asset::AssetInfo;
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

    mod deposit_helpers {
        use super::*;

        use cosmwasm_std::{coin, coins};
        use speculoos::prelude::*;

        fn deposit_fee_config(percent: u64) -> FeeConfig {
            FeeConfig {
                deposit: Decimal::percent(percent),
                performance: Decimal::zero(),
                withdrawal: Decimal::zero(),
                fee_collector_addr: Addr::unchecked("fee_collector"), // 10% fee
            }
        }

        #[test]
        fn consolidate_funds_matching_assets() {
            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let mut funds = vec![AnsAsset::new("coin", 0u128)];

            consolidate_funds(&mut funds, info_funds).unwrap();

            assert_that!(&funds).has_length(1);
            assert_that!(funds[0].amount).is_equal_to(Uint128::new(100));

            // fn consolidate_funds_equal_funds() {
            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let mut funds = vec![AnsAsset::new("coin", 100u128)];

            consolidate_funds(&mut funds, info_funds).unwrap();

            assert_that!(&funds).has_length(1);
            assert_that!(funds[0].amount).is_equal_to(Uint128::new(100));

            // fn consolidate_funds_all_zero() {
            let info_funds = vec![AnsAsset::new("coin", 0u128)];
            let mut funds = vec![AnsAsset::new("coin", 0u128)];

            consolidate_funds(&mut funds, info_funds).unwrap();

            assert_that!(&funds).has_length(1);
            assert_that!(funds[0].amount).is_equal_to(Uint128::new(0));

            // fn consolidate_funds_mismatch() {
            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let mut funds = vec![AnsAsset::new("coin", 10u128)];

            let res = consolidate_funds(&mut funds, info_funds);
            assert_that!(res).is_err();
        }

        #[test]
        fn consolidate_funds_non_matching_assets() {
            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let mut funds = vec![AnsAsset::new("coin2", 10u128)];
            let res = consolidate_funds(&mut funds, info_funds);
            assert_that!(res).is_err();

            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let funds = vec![];
            let res = consolidate_funds(&mut funds.clone(), info_funds);
            assert_that!(res).is_err();

            let info_funds = vec![AnsAsset::new("coin", 100u128)];
            let mut funds = vec![
                AnsAsset::new("coin", 100u128),
                AnsAsset::new("coin2", 10u128),
            ];
            consolidate_funds(&mut funds, info_funds).unwrap();
            assert_that!(funds).has_length(2);
            assert_that!(funds[0].amount).is_equal_to(Uint128::new(100));
            assert_that!(funds[1].amount).is_equal_to(Uint128::new(10));
        }

        #[test]
        fn transfer_token_to_proxy_ok() -> anyhow::Result<()> {
            let deps = app_init(false, true);
            let sender = Addr::unchecked("sender");

            let lp_token = Asset::new(AssetInfo::native("lp_token".to_string()), Uint128::new(100));

            let res = transfer_token_to_proxy(lp_token, sender, &AUTOCOMPOUNDER_APP, deps.as_ref());
            assert_that!(res).is_ok();
            assert!(matches!(res.unwrap(), CosmosMsg::Bank(_)));

            Ok(())
        }
        #[test]
        fn transfer_token_to_autocompounder_native() -> anyhow::Result<()> {
            let info = mock_info("sender", &coins(100, "lp_token"));
            let sender = info.sender.clone();
            let lp_token = Asset::new(AssetInfo::native("lp_token".to_string()), Uint128::new(100));
            let wanted_funds = Coin {
                denom: "lp_token".to_string(),
                amount: Uint128::new(100),
            }
            .to_string();

            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender.clone(),
                &mock_env(),
                &info.funds,
            );
            let msgs = assert_that!(res).is_ok();
            assert_that!(msgs.subject).has_length(0);

            // Check if it would fail if funds are not enough
            let info = mock_info("sender", &coins(99, "lp_token"));
            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender.clone(),
                &mock_env(),
                &info.funds,
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(AutocompounderError::FundsMismatch {
                    wanted_funds: wanted_funds.clone(),
                    sent_funds: info.funds[0].to_string(),
                });

            // Check if it would fail if funds are wrong denom
            let info = mock_info("sender", &coins(100, "wrong_denom"));
            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender.clone(),
                &mock_env(),
                &info.funds,
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(AutocompounderError::FundsMismatch {
                    wanted_funds: wanted_funds.clone(),
                    sent_funds: info.funds[0].to_string(),
                });

            // check if it would fail for multiple coins
            let info = mock_info(
                "sender",
                &vec![coin(100, "lp_token"), coin(33, "more_tokens")],
            );
            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender.clone(),
                &mock_env(),
                &info.funds,
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(AutocompounderError::FundsMismatch {
                    wanted_funds: wanted_funds.clone(),
                    sent_funds: info.funds[0].to_string(),
                });

            Ok(())
        }

        #[test]
        fn transfer_token_to_autocompounder_non_native() -> anyhow::Result<()> {
            let info = mock_info("sender", &coins(10, "lp_token"));
            let sender = info.sender.clone();
            let lp_token = Asset::new(
                AssetInfo::cw20(Addr::unchecked("lp_token")),
                Uint128::new(100),
            );

            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender.clone(),
                &mock_env(),
                &info.funds,
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(AutocompounderError::FundsMismatch {
                    wanted_funds: lp_token.to_string(),
                    sent_funds: info.funds[0].to_string(),
                });

            let info = mock_info("sender", &[]);
            let res = transfer_token_to_autocompounder(
                lp_token.clone(),
                sender,
                &mock_env(),
                &info.funds,
            );
            let msgs = assert_that!(res).is_ok();
            assert_that!(msgs.subject).has_length(1);

            Ok(())
        }

        #[test]
        fn add_asset_if_single_fund_adds_when_one_asset() {
            let config = min_cooldown_config(None, false);
            let mut funds = vec![AnsAsset::new("eur".to_string(), 100u128)];

            add_asset_if_single_fund(&mut funds, &config);

            assert_that!(funds).has_length(2);
            assert_that!(&funds).contains(&AnsAsset::new("eur".to_string(), 100u128));
            assert_that!(&funds).contains(&AnsAsset::new("usd".to_string(), 0u128));
        }

        #[test]
        fn add_asset_if_single_fund_does_nothing_when_multiple_assets() {
            let config = min_cooldown_config(None, false);
            let mut funds = vec![
                AnsAsset::new("eur".to_string(), 100u128),
                AnsAsset::new("usd".to_string(), 50u128),
            ];

            add_asset_if_single_fund(&mut funds, &config);

            assert_that!(&funds).contains(&AnsAsset::new("eur".to_string(), 100u128));
            assert_that!(&funds).contains(&AnsAsset::new("usd".to_string(), 50u128));
        }

        #[test]
        fn deduct_deposit_fees_applies_fee() {
            let mut funds = vec![AnsAsset::new("asset1".to_string(), 100u128)];
            let fee_config = deposit_fee_config(10);

            let fees = deduct_deposit_fees(&mut funds, &fee_config);

            assert_that!(funds[0].amount).is_equal_to(Uint128::from(90u128)); // 10% deducted
            assert_that!(&fees).contains(&AnsAsset::new("asset1".to_string(), 10u128));
        }

        #[test]
        fn deduct_deposit_fees_zero_fee() {
            let mut funds = vec![AnsAsset::new("asset1".to_string(), 100u128)];
            let fee_config = deposit_fee_config(0);

            let fees = deduct_deposit_fees(&mut funds, &fee_config);

            assert_that!(&funds[0].amount).is_equal_to(Uint128::from(100u128)); // No deduction
            assert_that!(&fees).is_empty();
        }

        #[test]
        fn check_all_funds_in_pool_all_present() {
            let funds = vec![AnsAsset::new("eur".to_string(), 100u128)];
            let config = min_cooldown_config(None, false);

            let result = check_all_funds_in_pool(&funds, &config);

            assert_that!(&result).is_ok();
        }

        #[test]
        fn check_all_funds_in_pool_some_absent() {
            let config = min_cooldown_config(None, false);
            let funds = vec![
                AnsAsset::new("eur".to_string(), 100u128),
                AnsAsset::new("asset3".to_string(), 50u128),
            ];

            let result = check_all_funds_in_pool(&funds, &config);

            assert_that!(&result)
                .is_err()
                .is_equal_to(AutocompounderError::AssetNotInPool {
                    asset: "asset3".to_string(),
                });
        }
    }

    mod fee_config {
        use speculoos::{assert_that, result::ResultAssertions};

        use crate::test_common::app_init;

        use super::*;

        #[test]
        fn only_admin() -> anyhow::Result<()> {
            let mut deps = app_init(false, true);
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
            let mut deps = app_init(false, true);
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
            let mut deps = app_init(false, true);
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
        use crate::msg::AutocompounderExecuteMsg;
        use crate::state::{Config, CONFIG};
        use crate::test_common::app_init;

        use super::*;

        #[test]
        fn update_staking_config_only_admin() -> anyhow::Result<()> {
            let mut deps = app_init(true, true);
            let msg = AutocompounderExecuteMsg::UpdateStakingConfig {
                bonding_data: Some(BondingData {
                    unbonding_period: Duration::Time(7200),
                    max_claims_per_address: None,
                }),
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
        let mut deps = app_init(false, true);
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
        let mut deps = app_init(true, true);
        let msg = AutocompounderExecuteMsg::Deposit {
            funds: vec![
                AnsAsset::new("eur", Uint128::one()),
                AnsAsset::new("juno", Uint128::one()),
            ],
            recipient: None,
            max_spread: None,
        };

        let wrong_coin = "juno".to_string();
        let resp = execute_as(
            deps.as_mut(),
            "user",
            msg,
            &[
                Coin::new(1u128, "eur"),
                Coin::new(1u128, wrong_coin.clone()),
            ],
        );
        assert_that!(resp).is_err();
        assert_that!(resp.unwrap_err())
            .is_equal_to(AutocompounderError::AssetNotInPool { asset: wrong_coin });
        Ok(())
    }

    #[test]
    fn cannot_withdraw_liquidity_if_no_claims() -> anyhow::Result<()> {
        let mut deps = app_init(true, true);
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

        let config = min_cooldown_config(Some(Duration::Time(60)), false);
        let env = mock_env();
        let result = check_unbonding_cooldown(&deps.as_mut(), &config, &env);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_unbonding_cooldown_with_expired_unbonding() {
        let mut deps = mock_dependencies();
        let config = min_cooldown_config(Some(Duration::Time(60)), false);
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
        let config = min_cooldown_config(Some(Duration::Time(60)), false);
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
        use cosmwasm_std::coins;

        use crate::test_common::TEST_VAULT_TOKEN;

        use super::*;

        #[test]
        fn test_register_pre_claim() {
            let mut deps = mock_dependencies();

            let sender = String::from("sender");
            let sender_addr = Addr::unchecked(sender);
            let amount_of_vault_tokens_to_be_burned = Uint128::new(100);

            // Test case when there is no pending claim for the sender
            let res = register_pre_claim(
                deps.as_mut(),
                sender_addr.clone(),
                amount_of_vault_tokens_to_be_burned,
            );
            assert_that!(res).is_ok();

            let pending_claim = PENDING_CLAIMS
                .load(deps.as_ref().storage, sender_addr.clone())
                .unwrap();
            assert_that!(pending_claim).is_equal_to(amount_of_vault_tokens_to_be_burned);

            // Test case when there is a pending claim for the sender
            let amount_of_vault_tokens_to_be_burned_2 = Uint128::new(200);
            let res = register_pre_claim(
                deps.as_mut(),
                sender_addr.clone(),
                amount_of_vault_tokens_to_be_burned_2,
            );
            assert_that!(res).is_ok();

            let pending_claim = PENDING_CLAIMS
                .load(deps.as_ref().storage, sender_addr)
                .unwrap();
            assert_that!(pending_claim).is_equal_to(
                amount_of_vault_tokens_to_be_burned + amount_of_vault_tokens_to_be_burned_2,
            );
        }

        #[test]
        fn receive_and_register_native() -> anyhow::Result<()> {
            let mut deps = app_init(true, true);
            let config = min_cooldown_config(Some(Duration::Height(1)), true);
            CONFIG.save(deps.as_mut().storage, &config)?;
            let env = &mock_env();
            let info = mock_info("sender", &coins(100, TEST_VAULT_TOKEN));
            let sender = info.sender.clone();

            let amount = Uint128::new(100);

            let res = receive_and_register_claim(
                deps.as_mut(),
                env,
                AUTOCOMPOUNDER_APP,
                sender.clone(),
                info.clone(),
                amount,
            )?;
            assert_that!(res.messages).has_length(0);

            Ok(())
        }

        #[test]
        fn receive_and_register_cw20() -> anyhow::Result<()> {
            let mut deps = app_init(true, true);
            let config = min_cooldown_config(Some(Duration::Height(1)), false);
            CONFIG.save(deps.as_mut().storage, &config)?;
            let env = &mock_env();
            let info = mock_info("sender", &[]);
            let sender = info.sender.clone();

            let amount = Uint128::new(100);
            let lp_asset_base = AssetBase::new(config.vault_token, amount);

            let res = receive_and_register_claim(
                deps.as_mut(),
                env,
                AUTOCOMPOUNDER_APP,
                sender.clone(),
                info.clone(),
                amount,
            )?;
            assert_that!(res.messages).has_length(1);
            assert_that!(res.messages[0].msg).is_equal_to(
                transfer_token_to_autocompounder(lp_asset_base, sender, env, &info.funds)?
                    .first()
                    .unwrap(),
            );

            Ok(())
        }

        #[test]

        fn without_bonding_period_cw20() -> anyhow::Result<()> {
            // NOTE: This test cant be done with native tokens because itll trigger a stargate query which is not supported in the test environment
            let mut deps = app_init(false, true);
            let config = min_cooldown_config(None, false);
            let env = &mock_env();
            let info = mock_info("sender", &[]);
            let sender = info.sender.clone();
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
                env,
                &sender,
                info,
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
                .map(|(k, v)| (k, v))
                .collect();
            assert_that!(cached_assets).has_length(2);
            assert_that!(cached_assets[0]).is_equal_to(("native:eur".to_string(), 1000u128.into()));
            assert_that!(cached_assets[1]).is_equal_to(("native:usd".to_string(), 1000u128.into()));

            // The contract should have sent the correct messages
            assert_that!(response.messages).has_length(4);
            assert_that!(response.messages[1].msg).is_equal_to(unstake_lp_tokens(
                deps.as_ref(),
                &AUTOCOMPOUNDER_APP,
                config.pool_data.dex,
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
    }

    mod deposit_recipient {
        use super::*;

        #[test]
        fn deposit_recipient() -> anyhow::Result<()> {
            let mut app = app_init(false, true);
            let config = min_cooldown_config(None, false);
            let fee_config = FeeConfig {
                deposit: Decimal::percent(10),
                performance: Decimal::percent(10),
                withdrawal: Decimal::percent(10),
                fee_collector_addr: Addr::unchecked("fee_collector"),
            };
            CONFIG.save(&mut app.storage, &config)?;
            FEE_CONFIG.save(&mut app.storage, &fee_config)?;

            // this is just to make the rest of the
            let info = mock_info("sender", &[]);
            let lp_asset = AnsAsset::new("eur_usd_lp", Uint128::new(100));
            let not_lp_asset = AnsAsset::new("noteur_usd_lp", Uint128::new(100));

            // this positive test case is not persee needed, but it's here as a sanity check
            assert_that!(deposit_lp(
                app.as_mut(),
                info.clone(),
                mock_env(),
                AUTOCOMPOUNDER_APP,
                lp_asset.clone(),
                None
            ))
            .is_ok();

            assert_that!(deposit_lp(
                app.as_mut(),
                info,
                mock_env(),
                AUTOCOMPOUNDER_APP,
                not_lp_asset,
                None
            ))
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::SenderIsNotLpToken {}));
            Ok(())
        }

        #[test]
        fn unwrap_recipient() -> anyhow::Result<()> {
            let not_allowed = vec![
                Addr::unchecked("addr1".to_string()),
                Addr::unchecked("addr2".to_string()),
                Addr::unchecked("addr3".to_string()),
            ];
            let sender = Addr::unchecked("sender".to_string());

            let res = unwrap_recipient_is_allowed(None, &sender, not_allowed.clone())?;
            assert_that!(res).is_equal_to(sender.clone());

            let res =
                unwrap_recipient_is_allowed(Some(sender.clone()), &sender, not_allowed.clone())?;
            assert_that!(res).is_equal_to(sender.clone());

            let res = unwrap_recipient_is_allowed(
                Some(not_allowed[0].clone()),
                &sender,
                not_allowed.clone(),
            );
            assert_that!(res).is_err();

            let res = unwrap_recipient_is_allowed(Some(not_allowed[1].clone()), &sender, vec![])?;
            assert_that!(res).is_equal_to(not_allowed[1].clone());

            Ok(())
        }
    }
}
