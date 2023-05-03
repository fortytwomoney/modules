mod common;

use std::ops::Mul;
use std::str::FromStr;

use abstract_boot::{Abstract, AbstractBootError, ManagerQueryFns, VCExecFns};
use abstract_core::api::{BaseExecuteMsgFns, BaseQueryMsgFns};
use abstract_core::objects::{AnsAsset, AssetEntry};
use abstract_sdk::core as abstract_core;

use abstract_cw_staking_api::CW_STAKING;
use abstract_dex_api::EXCHANGE;
use autocompounder::state::{Claim, Config, DECIMAL_OFFSET, PENDING_CLAIMS, FeeConfig};
use boot_core::*;
use boot_cw_plus::Cw20Base;
use boot_cw_plus::Cw20ExecuteMsgFns;
use boot_cw_plus::Cw20QueryMsgFns;

use autocompounder::msg::{
    AutocompounderExecuteMsg, AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns,
    BondingPeriodSelector,
};

use autocompounder::msg::{Cw20HookMsg, AUTOCOMPOUNDER};
use common::abstract_helper::{self, init_auto_compounder};
use common::vault::Vault;
use common::AResult;
use cosmwasm_std::{coin, coins, to_binary, Addr, Decimal, Empty, Uint128};

use cw_utils::{Duration, Expiration};
use speculoos::assert_that;
use speculoos::prelude::OrderedAssertions;
use wyndex_stake::msg::ReceiveDelegationMsg;

use speculoos::result::ResultAssertions;
use wyndex_bundle::*;

const WYNDEX: &str = "wyndex";
const COMMISSION_RECEIVER: &str = "commission_receiver";
const VAULT_TOKEN: &str = "vault_token";
const TEST_NAMESPACE: &str = "4t2";
const ATTACKER: &str = "attacker";
/// Convert vault tokens to lp assets
pub fn convert_to_assets(
    shares: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    let shares = shares.multiply_ratio(
        total_assets + Uint128::from(1u128),
        total_supply + Uint128::from(10u128).pow(decimal_offset),
    );
    shares
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
pub fn convert_to_shares(
    assets: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    let assets = assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(decimal_offset),
        total_assets + Uint128::from(1u128),
    );
    assets
}

fn create_vault(mock: Mock) -> Result<Vault<Mock>, AbstractBootError> {
    let version = "1.0.0".parse().unwrap();
    // Deploy abstract
    let abstract_ = Abstract::deploy_on(mock.clone(), version)?;
    // create first Account
    abstract_.account_factory.create_default_account(
        abstract_core::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;

    abstract_
        .version_control
        .claim_namespaces(0, vec![TEST_NAMESPACE.to_string()])?;

    // Deploy mock dex
    let wyndex = WynDex::store_on(mock.clone()).unwrap();

    let eur_asset = AssetEntry::new(EUR);
    let usd_asset = AssetEntry::new(USD);

    // Set up the dex and staking contracts
    let exchange_api = abstract_helper::init_exchange(mock.clone(), &abstract_, None)?;
    let staking_api = abstract_helper::init_staking(mock.clone(), &abstract_, None)?;
    let auto_compounder = init_auto_compounder(mock.clone(), &abstract_, None)?;

    let mut vault_token = Cw20Base::new(VAULT_TOKEN, mock.clone());
    // upload the vault token code
    let vault_toke_code_id = vault_token.upload()?.uploaded_code_id()?;
    // Create an Account that we will turn into a vault
    let account = abstract_.account_factory.create_default_account(
        abstract_core::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;

    // install dex
    account.manager.install_module(EXCHANGE, &Empty {})?;
    // install staking
    account.manager.install_module(CW_STAKING, &Empty {})?;
    // install autocompounder
    account.manager.install_module(
        AUTOCOMPOUNDER,
        &abstract_core::app::InstantiateMsg {
            module: autocompounder::msg::AutocompounderInstantiateMsg {
                code_id: vault_toke_code_id,
                commission_addr: COMMISSION_RECEIVER.to_string(),
                deposit_fees: Decimal::percent(0),
                dex: WYNDEX.to_string(),
                fee_asset: eur_asset.to_string(),
                performance_fees: Decimal::percent(3),
                pool_assets: vec![eur_asset, usd_asset],
                withdrawal_fees: Decimal::percent(0),
                preferred_bonding_period: BondingPeriodSelector::Shortest,
                max_swap_spread: None,
            },
            base: abstract_core::app::BaseInstantiateMsg {
                ans_host_address: abstract_.ans_host.addr_str()?,
            },
        },
    )?;
    // get its address
    let auto_compounder_addr = account
        .manager
        .module_addresses(vec![AUTOCOMPOUNDER.into()])?
        .modules[0]
        .1
        .clone();
    // set the address on the contract
    auto_compounder.set_address(&Addr::unchecked(auto_compounder_addr.clone()));

    // give the autocompounder permissions to call on the dex and cw-staking contracts
    exchange_api
        .call_as(&account.manager.address()?)
        .update_authorized_addresses(vec![auto_compounder_addr.clone()], vec![])?;
    staking_api
        .call_as(&account.manager.address()?)
        .update_authorized_addresses(vec![auto_compounder_addr], vec![])?;

    // set the vault token address
    let auto_compounder_config = auto_compounder.config()?;
    vault_token.set_address(&auto_compounder_config.vault_token);

    Ok(Vault {
        account,
        auto_compounder,
        vault_token,
        abstract_core: abstract_,
        wyndex,
        dex: exchange_api,
        staking: staking_api,
    })
}

#[test]
fn proper_initialisation() {
    // initialize with non existing pair
    // initialize with non existing fee token
    // initialize with non existing reward token
    // initialize with no pool for the fee token and reward token
}

/// This test covers:
/// - Create a vault and check its configuration setup.
/// - Deposit balanced funds into the auto-compounder and check the minted vault token.
/// - Withdraw a part from the auto-compounder and check the pending claims.
/// - Check that the pending claims are updated after another withdraw.
/// - Batch unbond and check the pending claims are removed.
/// - Withdraw and check the removal of claims.
/// - Check the balances and staked balances.
/// - Withdraw all from the auto-compounder and check the balances again.
#[test]
fn generator_without_reward_proxies_balanced_assets() -> AResult {
    let owner = Addr::unchecked(common::OWNER);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        eur_usd_lp,
        ..
    } = vault.wyndex;
    let vault_token = vault.vault_token;
    let auto_compounder_addr = vault.auto_compounder.addr_str()?;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");
    let asset_infos = vec![eur_token.clone(), usd_token.clone()];

    // check config setup
    let config = vault.auto_compounder.config()?;
    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address()?);

    // give user some funds
    mock.set_balances(&[(
        &owner,
        &[
            coin(100_000u128, eur_token.to_string()),
            coin(100_000u128, usd_token.to_string()),
        ],
    )])?;

    // initial deposit must be > 1000 (of both assets)
    // this is set by WynDex
    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(eur_asset, 10000u128),
            AnsAsset::new(usd_asset, 10000u128),
        ],
        &[coin(10000u128, EUR), coin(10000u128, USD)],
    )?;

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(owner.to_string())?;
    assert_that!(vault_token_balance.balance.u128()).is_equal_to(100000u128);

    // and eur balance decreased and usd balance stayed the same
    let balances = mock.query_all_balances(&owner)?;

    // .sort_by(|a, b| a.denom.cmp(&b.denom));
    assert_that!(balances).is_equal_to(vec![
        coin(90_000u128, eur_token.to_string()),
        coin(90_000u128, usd_token.to_string()),
    ]);

    // withdraw part from the auto-compounder
    vault_token.send(
        Uint128::from(20000u128),
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;
    // check that the vault token decreased
    let vault_token_balance = vault_token.balance(owner.to_string())?;
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(vault_token_balance.balance.u128()).is_equal_to(80000u128);
    assert_that!(pending_claims.u128()).is_equal_to(20000u128);

    // check that the pending claims are updated
    vault_token.send(
        Uint128::from(20000u128),
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(pending_claims.u128()).is_equal_to(40000u128);

    vault.auto_compounder.batch_unbond(None, None)?;

    // checks if the pending claims are now removed
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(pending_claims.u128()).is_equal_to(0u128);

    mock.next_block()?;
    let claims = vault.auto_compounder.claims(owner.to_string())?;
    let unbonding: Expiration = claims[0].unbonding_timestamp;
    if let Expiration::AtTime(time) = unbonding {
        mock.app.borrow_mut().update_block(|b| {
            b.time = time.plus_seconds(10);
        });
    }
    mock.next_block()?;
    vault.auto_compounder.withdraw()?;

    // check that the claim is removed
    let claims: Vec<Claim> = vault.auto_compounder.claims(owner.to_string())?;
    assert_that!(claims.len()).is_equal_to(0);

    let balances = mock.query_all_balances(&owner)?;
    // .sort_by(|a, b| a.denom.cmp(&b.denom));
    assert_that!(balances).is_equal_to(vec![
        coin(94_000u128, eur_token.to_string()),
        coin(94_000u128, usd_token.to_string()),
    ]);

    let staked = vault
        .wyndex
        .suite
        .query_all_staked(asset_infos, &vault.account.proxy.addr_str()?)?;

    let generator_staked_balance = staked.stakes.first().unwrap();
    assert_that!(generator_staked_balance.stake.u128()).is_equal_to(6000u128);

    // withdraw all from the auto-compounder
    vault_token.send(
        Uint128::from(60000u128),
        auto_compounder_addr,
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;
    vault.auto_compounder.batch_unbond(None, None)?;
    mock.wait_blocks(60 * 60 * 24 * 21)?;
    vault.auto_compounder.withdraw()?;

    // and eur balance decreased and usd balance stayed the same
    let balances = mock.query_all_balances(&owner)?;

    // .sort_by(|a, b| a.denom.cmp(&b.denom));
    assert_that!(balances).is_equal_to(vec![
        coin(100_000u128, eur_token.to_string()),
        coin(100_000u128, usd_token.to_string()),
    ]);
    Ok(())
}

/// This test covers:
/// - depositing with 2 assets
/// - depositing and withdrawing with a single sided asset
/// - querying the state of the auto-compounder
/// - querying the balance of a users position in the auto-compounder
/// - querying the total lp balance of the auto-compounder
/// - draining vault funds by owner before user.
#[test]
fn generator_without_reward_proxies_single_sided() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let user1: Addr = Addr::unchecked(common::USER1);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let mut vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        eur_usd_lp,
        ..
    } = vault.wyndex;
    let mut vault_token = vault.vault_token;
    let auto_compounder_addr = vault.auto_compounder.addr_str()?;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");
    let asset_infos = vec![eur_token.clone(), usd_token.clone()];

    // check config setup
    let config: Config = vault.auto_compounder.config()?;
    let position = vault.auto_compounder.total_lp_position()?;
    assert_that!(position).is_equal_to(Uint128::zero());

    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address()?);

    // give user some funds
    mock.set_balances(&[
        (
            &owner,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
        (
            &user1,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
    ])?;

    // initial deposit must be > 1000 (of both assets)
    // this is set by WynDex
    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(eur_asset.clone(), 10000u128),
            AnsAsset::new(usd_asset.clone(), 10000u128),
        ],
        &[coin(10_000u128, EUR), coin(10_000u128, USD)],
    )?;

    let position = vault.auto_compounder.total_lp_position()?;
    assert_that!(position).is_equal_to(Uint128::from(10_000u128));

    let balance_owner = vault_token.balance(owner.to_string())?;
    assert_that!(balance_owner.balance.u128()).is_equal_to(10_000u128 * 10u128.pow(DECIMAL_OFFSET));

    // single asset deposit from different address
    vault.auto_compounder.set_sender(&user1);
    vault.auto_compounder.deposit(
        vec![AnsAsset::new(eur_asset, 1000u128)],
        &[coin(1000u128, EUR)],
    )?;

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(owner.to_string())?;
    assert_that!(vault_token_balance.balance.u128())
        .is_equal_to(10000u128 * 10u128.pow(DECIMAL_OFFSET));
    let new_position = vault.auto_compounder.total_lp_position()?;
    // check if the user1 balance is correct
    let vault_token_balance_user1 = vault_token.balance(user1.to_string())?;
    assert_that!(vault_token_balance_user1.balance.u128())
        .is_equal_to(487u128 * 10u128.pow(DECIMAL_OFFSET));
    assert_that!(new_position).is_greater_than(position);

    vault.auto_compounder.deposit(
        vec![AnsAsset::new(usd_asset, 1000u128)],
        &[coin(1000u128, USD)],
    )?;

    // check that the vault owner balance remains the same
    let vault_token_balance = vault_token.balance(owner.to_string())?.balance;
    assert_that!(vault_token_balance.u128()).is_equal_to(10000u128 * 10u128.pow(DECIMAL_OFFSET));
    // check if the user1 balance is correct
    let vault_token_balance_user1 = vault_token.balance(user1.to_string())?.balance;
    assert_that!(vault_token_balance_user1.u128())
        .is_equal_to(986u128 * 10u128.pow(DECIMAL_OFFSET));

    // check if the vault balance query functions properly:
    let vault_balance_queried = vault.auto_compounder.balance(owner.to_string())?;
    assert_that!(vault_balance_queried).is_equal_to(Uint128::from(vault_token_balance.u128()));

    let vault_balance_queried = vault.auto_compounder.balance(user1.to_string())?;
    assert_that!(vault_balance_queried)
        .is_equal_to(Uint128::from(vault_token_balance_user1.u128()));

    let position = new_position;
    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_greater_than(position);

    // and eur balance decreased and usd balance decreased
    let owner_balances = mock.query_all_balances(&owner)?;
    assert_that!(owner_balances).is_equal_to(vec![
        coin(90_000u128, eur_token.to_string()),
        coin(90_000u128, usd_token.to_string()),
    ]);
    let user1_balances = mock.query_all_balances(&user1)?;
    assert_that!(user1_balances).is_equal_to(vec![
        coin(99_000u128, eur_token.to_string()),
        coin(99_000u128, usd_token.to_string()),
    ]);

    // calculate how much lp tokens the user should get if he withdraws everything before anyone withdraws
    let vault_token_balance = vault_token.balance(user1.to_string())?.balance;
    let total_supply = vault_token.token_info()?.total_supply;
    let user1_lp_tokens_voucher =
        convert_to_assets(vault_token_balance, new_position, total_supply, 0u32);

    // withdraw part from the auto-compounder
    vault.auto_compounder.set_sender(&owner);
    vault_token.send(
        Uint128::from(4000u128 * 10u128.pow(DECIMAL_OFFSET)),
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;
    // check that the vault token decreased
    let vault_token_balance = vault_token.balance(owner.to_string())?;
    assert_that!(vault_token_balance.balance.u128())
        .is_equal_to(6000u128 * 10u128.pow(DECIMAL_OFFSET));

    let pending_claim = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(pending_claim.u128()).is_equal_to(4000u128 * 10u128.pow(DECIMAL_OFFSET));
    let vault_token_balance = vault_token.balance(vault.auto_compounder.address()?.to_string())?;
    assert_that!(vault_token_balance.balance.u128())
        .is_equal_to(4000u128 * 10u128.pow(DECIMAL_OFFSET));

    let total_lp_balance = vault.auto_compounder.total_lp_position()?;
    assert_that!(total_lp_balance).is_equal_to(new_position);

    let generator_staked_balance = vault
        .wyndex
        .suite
        .query_all_staked(asset_infos.clone(), &vault.account.proxy.addr_str()?)?
        .stakes[0]
        .stake;
    assert_that!(generator_staked_balance.u128()).is_equal_to(10986u128);

    // Batch unbond pending claims
    vault.auto_compounder.batch_unbond(None, None)?;

    // query the claims of the auto-compounder
    let claims = vault.auto_compounder.claims(owner.to_string())?;
    let expected_claim = Claim {
        unbonding_timestamp: Expiration::AtTime(mock.block_info()?.time.plus_seconds(1)),
        amount_of_vault_tokens_to_burn: (4000u128 * 10u128.pow(DECIMAL_OFFSET)).into(),
        amount_of_lp_tokens_to_unbond: 4000u128.into(), // 1 lp token is accuired by the virtual assets
    };
    assert_that!(claims).is_equal_to(vec![expected_claim]);

    // let the time pass and withdraw the claims
    mock.wait_blocks(60 * 60 * 24 * 10)?;

    // let total_lp_balance = vault.auto_compounder.total_lp_position()?;
    // assert_that!(total_lp_balance).is_equal_to(new_position);
    vault.auto_compounder.withdraw()?;

    // and eur and usd balance increased
    let balances = mock.query_all_balances(&owner)?;
    assert_that!(balances).is_equal_to(vec![
        coin(94_002u128, eur_token.to_string()),
        coin(94_002u128, usd_token.to_string()),
    ]);

    let position = new_position;
    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_less_than(position);

    let prev_generator_staked_balance = generator_staked_balance;
    let generator_staked_balance = vault
        .wyndex
        .suite
        .query_all_staked(asset_infos, &vault.account.proxy.addr_str()?)?
        .stakes[0]
        .stake;
    assert_that!(generator_staked_balance.u128())
        .is_equal_to(prev_generator_staked_balance.u128() - 4000u128);

    // withdraw all owner funds from the auto-compounder
    vault_token.send(
        Uint128::from(6000u128 * 10u128.pow(DECIMAL_OFFSET)),
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    // testing general non unbonding staking contract functionality
    let pending_claims = vault
        .auto_compounder
        .pending_claims(owner.to_string())?
        .into();
    assert_that!(pending_claims).is_equal_to(6000u128 * 10u128.pow(DECIMAL_OFFSET)); // no unbonding period, so no pending claims

    vault.auto_compounder.batch_unbond(None, None)?; // batch unbonding not enabled
    mock.wait_blocks(60 * 60 * 24 * 10)?;
    vault.auto_compounder.withdraw()?; // withdraw wont have any effect, because there are no pending claims
                                       // mock.next_block()?;

    let balances = mock.query_all_balances(&owner)?;
    assert_that!(balances).is_equal_to(vec![
        // coin(99_993u128, eur_token.to_string()),
        // coin(99_993u128, usd_token.to_string()),
        coin(100_006u128, eur_token.to_string()),
        coin(100_006u128, usd_token.to_string()),
    ]);

    // Withdraw user1 funds
    let prev_vault_token_balance_user1 = vault_token_balance_user1;
    let vault_token_balance_user1 = vault_token.balance(user1.to_string())?.balance;
    assert_that!(vault_token_balance_user1.u128())
        .is_equal_to(prev_vault_token_balance_user1.u128());

    vault.auto_compounder.set_sender(&user1);
    vault_token.set_sender(&user1);
    vault_token.send(
        vault_token_balance_user1,
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    let pending_claims = vault
        .auto_compounder
        .pending_claims(user1.to_string())?
        .into();
    assert_that!(pending_claims).is_equal_to(vault_token_balance_user1.u128());

    vault.auto_compounder.batch_unbond(None, None)?;

    let claims = vault.auto_compounder.claims(user1.to_string())?;
    let expected_claim = Claim {
        unbonding_timestamp: Expiration::AtTime(mock.block_info()?.time.plus_seconds(1)),
        amount_of_vault_tokens_to_burn: vault_token_balance_user1.into(),
        amount_of_lp_tokens_to_unbond: user1_lp_tokens_voucher,
    };
    assert_that!(claims).is_equal_to(vec![expected_claim]);

    mock.wait_blocks(60 * 60 * 24 * 10)?;
    vault.auto_compounder.withdraw()?;
    // mock.next_block()?;
    // a relative loss is experienced by the user due to swap fees and drainage of the pool to 0
    let balances = mock.query_all_balances(&user1)?;
    assert_that!(balances).is_equal_to(vec![
        coin(99_986u128, eur_token.to_string()),
        coin(99_986u128, usd_token.to_string()),
    ]);

    Ok(())
}

/// This test covers the following scenario:
/// - create a pool with rewards
/// - deposit into the pool in-balance
/// - compound rewards
/// - checks if the fee distribution is correct
/// - checks if the rewards are distributed correctly
#[test]
fn generator_with_rewards_test_fee_and_reward_distribution() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let commission_addr = Addr::unchecked(COMMISSION_RECEIVER);
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let mut vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        eur_usd_lp,
        eur_usd_staking,
        ..
    } = vault.wyndex;

    let vault_token = vault.vault_token;
    let auto_compounder_addr = vault.auto_compounder.addr_str()?;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");

    // check config setup
    let config = vault.auto_compounder.config()?;
    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address()?);

    // give user some funds
    mock.set_balances(&[
        (
            &owner,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
        (&wyndex_owner, &[coin(100_000u128, WYND_TOKEN.to_string())]),
    ])?;

    // initial deposit must be > 1000 (of both assets)
    // this is set by WynDex
    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(eur_asset, 100_000u128),
            AnsAsset::new(usd_asset, 100_000u128),
        ],
        &[coin(100_000u128, EUR), coin(100_000u128, USD)],
    )?;

    // query how much lp tokens are in the vault
    let vault_lp_balance = vault.auto_compounder.total_lp_position()? as Uint128;

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(owner.to_string())?;
    assert_that!(vault_token_balance.balance.u128())
        .is_equal_to(100_000u128 * 10u128.pow(DECIMAL_OFFSET));
    let ownerbalance = mock.query_balance(&owner, EUR)?;
    assert_that!(ownerbalance.u128()).is_equal_to(0u128);

    // process block -> the AC should have pending rewards at the staking contract
    mock.next_block()?;
    vault.wyndex.suite.distribute_funds(
        eur_usd_staking,
        wyndex_owner.as_str(),
        &coins(1000, WYND_TOKEN),
    )?; // distribute 1000 EUR

    // rewards are 1_000 WYND each block for the entire amount of staked lp.
    // the fee received should be equal to 3% of the rewarded tokens which is then swapped using the astro/EUR pair.
    // the fee is 3% of 1K = 30, rewards are then 970
    // the fee is then swapped using the astro/EUR pair
    // the price of the WYND/EUR pair is 10K:10K
    // which will result in a 29 EUR fee for the autocompounder due to spread + rounding.
    vault.auto_compounder.compound()?;

    let commission_received: Uint128 = mock.query_balance(&commission_addr, EUR)?;
    assert_that!(commission_received.u128()).is_equal_to(29u128);

    // The reward for the user is then 970 WYND which is then swapped using the WYND/EUR pair
    // this will be swapped for ~880 EUR, which then is provided using single sided provide_liquidity
    let new_vault_lp_balance: Uint128 = vault.auto_compounder.total_lp_position()?;
    let new_lp: Uint128 = new_vault_lp_balance - vault_lp_balance;
    let expected_new_value: Uint128 = Uint128::from(vault_lp_balance.u128() * 4u128 / 1000u128); // 0.4% of the previous position
    assert_that!(new_lp).is_greater_than(expected_new_value);

    let owner_balance_eur = mock.query_balance(&owner, EUR)?;
    let owner_balance_usd = mock.query_balance(&owner, USD)?;

    // Redeem vault tokens and create pending claim of user tokens to see if the user actually received more of EUR and USD then they deposited
    vault_token.send(
        vault_token_balance.balance,
        auto_compounder_addr,
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    // Unbond tokens & clear pending claims
    vault.auto_compounder.batch_unbond(None, None)?;

    mock.wait_blocks(1)?;

    // Withdraw EUR and USD tokens to user
    vault.auto_compounder.withdraw()?;

    let new_owner_balance = mock.query_all_balances(&owner)?;
    let eur_diff = new_owner_balance[0].amount.u128() - owner_balance_eur.u128();
    let usd_diff = new_owner_balance[1].amount.u128() - owner_balance_usd.u128();

    // the user should have received more of EUR and USD then they deposited
    assert_that!(eur_diff).is_greater_than(100_000u128); // estimated value
    assert_that!(usd_diff).is_greater_than(100_000u128);

    Ok(())
}

#[test]
fn test_deposit_and_withdrawal_fees() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let commission_addr = Addr::unchecked(COMMISSION_RECEIVER);
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);
    let (_, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let mut vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        eur_usd_staking,

        ..
    } = vault.wyndex;
    let vault_token = vault.vault_token;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");
    // give user some funds
    mock.set_balances(&[
        (
            &owner,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
    ])?;
    // update performance fees to zero and deposit/withdrawal fees to 10%
    let manager_addr = vault.account.manager.address()?;
    vault.auto_compounder.call_as(&manager_addr).execute_app(
        AutocompounderExecuteMsg::UpdateFeeConfig {
            performance: Some(Decimal::zero()),
            deposit: Some(Decimal::from_str("0.1")?),
            withdrawal: Some(Decimal::from_str("0.1")?),
        },
        None,
    )?;

    let fee_config: FeeConfig = vault.auto_compounder.fee_config()?;
    assert_that!(fee_config.deposit).is_equal_to(Decimal::from_str("0.1")?);
    assert_that!(fee_config.withdrawal).is_equal_to(Decimal::from_str("0.1")?);
    assert_that!(fee_config.performance).is_equal_to(Decimal::zero());


    // deposit 1000 EUR
    vault.auto_compounder.deposit(vec![
        AnsAsset::new(eur_asset.clone(), 100_000u128),
        AnsAsset::new(usd_asset.clone(), 100_000u128),
    ], &[coin(100_000u128, EUR), coin(100_000u128, USD)])?;

    // deposit should be 10% less due to deposit fee
    let new_vault_lp_balance: Uint128 = vault.auto_compounder.total_lp_position()?;
    
    let expected_new_value: Uint128 = new_vault_lp_balance * fee_config.deposit; // 90% of the previous position
    let owner_balance = vault_token.balance(owner.to_string())?.balance;
    assert_that!(owner_balance).is_equal_to(expected_new_value);
    

    vault_token.send(
        owner_balance,
        vault.auto_compounder.addr_str()?,
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    let amount: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(amount).is_equal_to(owner_balance);

    // Unbond tokens & clear pending claims
    vault.auto_compounder.batch_unbond(None, None)?;

    mock.wait_blocks(60 * 60 * 24 * 10)?;
    vault.auto_compounder.withdraw()?;

    // TODO: Recompute the expected amount here
    let new_owner_balance = mock.query_all_balances(&owner)?;
    assert_that!(new_owner_balance[0].amount.u128()).is_equal_to(81_000u128); // estimated value
    assert_that!(new_owner_balance[1].amount.u128()).is_equal_to(81_000u128); // estimated value


    Ok(())
}

#[test]
fn test_zero_performance_fees() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let commission_addr = Addr::unchecked(COMMISSION_RECEIVER);
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);
    let (_, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let mut vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        eur_usd_staking,
        ..
    } = vault.wyndex;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");
    // give user some funds
    mock.set_balances(&[
        (
            &owner,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
        (&wyndex_owner, &[coin(1000, WYND_TOKEN)]),
    ])?;
    // update performance fees to zero
    let manager_addr = vault.account.manager.address()?;
    vault.auto_compounder.call_as(&manager_addr).execute_app(
        AutocompounderExecuteMsg::UpdateFeeConfig {
            performance: Some(Decimal::zero()),
            deposit: None,
            withdrawal: None,
        },
        None,
    )?;

    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(eur_asset, 100_000u128),
            AnsAsset::new(usd_asset, 100_000u128),
        ],
        &[coin(100_000u128, EUR), coin(100_000u128, USD)],
    )?;

    mock.next_block()?;
    vault.wyndex.suite.distribute_funds(
        eur_usd_staking,
        wyndex_owner.as_str(),
        &coins(1000, WYND_TOKEN),
    )?; // distribute 1000 EUR

    vault.auto_compounder.compound()?;
    let commission_received: Uint128 = mock.query_balance(&commission_addr, EUR)?;
    assert_that!(commission_received.u128()).is_equal_to(0u128);
    Ok(())
}

#[test]
fn test_owned_funds_stay_in_vault() -> AResult {
    // test that the funds in the vault are not used for the autocompounding and fee reward distribution
    let owner = Addr::unchecked(common::OWNER);
    let (_, mock) = instantiate_default_mock_env(&owner)?;
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);
    let vault = crate::create_vault(mock.clone())?;
    let WynDex {
        eur_token,
        usd_token,
        ..
    } = vault.wyndex;
    let vault_token = vault.vault_token;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");

    // give user some funds
    mock.set_balances(&[
        (
            &owner,
            &[
                coin(100_000u128, eur_token.to_string()),
                coin(100_000u128, usd_token.to_string()),
            ],
        ),
        (&wyndex_owner, &[coin(1000, WYND_TOKEN)]),
    ])?;

    // Fee asset is EUR
    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(eur_asset, 100_000u128),
            AnsAsset::new(usd_asset, 100_000u128),
        ],
        &[coin(100_000u128, EUR), coin(100_000u128, USD)],
    )?;

    // NOTE: The following commented block shows that the compound function also consumes all funds it has available.
    // The 3rd audit point was about this, but not in compound. It might even be desired behaviour that
    // the vault just compounds all funds it has available. this is in favour of users(?)

    // mock.next_block()?;
    // vault.wyndex.suite.distribute_funds(
    //     eur_usd_staking,
    //     wyndex_owner.as_str(),
    //     &coins(1000, WYND_TOKEN),
    // )?; // distribute 1000 EUR

    // mock.set_balance(&vault.account.proxy.address()?, coins(100_000u128, EUR))?;
    // mock.set_balance(&vault.account.proxy.address()?, coins(100_000u128, USD))?;

    // vault.auto_compounder.compound()?; // this will call fee_swapped_reply
    // let vault_eur_balance = mock.query_balance(&vault.account.proxy.address()?, EUR)?;
    // // let vault_usd_balance = mock.query_balance(&vault.account.proxy.address()?, USD)?;
    // assert_that!(vault_eur_balance.u128()).is_equal_to(100_000u128);
    // assert_that!(vault_usd_balance.u128()).is_equal_to(100_000u128);

    let owner_vault_tokens = vault_token.balance(owner.to_string())?.balance;
    vault_token.send(
        owner_vault_tokens,
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    // Unbond tokens & clear pending claims
    vault.auto_compounder.batch_unbond(None, None)?;
    mock.wait_blocks(60 * 60 * 24 * 10)?;

    // Send EUR to the autocompounder
    mock.set_balance(
        &vault.account.proxy.address()?,
        vec![
            coin(100_000u128, EUR),
            coin(100_000u128, USD),
            coin(100_000u128, WYND_TOKEN),
        ],
    )?;

    // Withdraw EUR and USD tokens to user
    vault.auto_compounder.withdraw()?; // this will call lp_withdraw_reply

    let vault_eur_balance = mock.query_balance(&vault.account.proxy.address()?, EUR)?;
    let vault_usd_balance = mock.query_balance(&vault.account.proxy.address()?, USD)?;
    let vault_wynd_balance = mock.query_balance(&vault.account.proxy.address()?, WYND_TOKEN)?;
    assert_that!(vault_eur_balance.u128()).is_equal_to(100_000u128);
    assert_that!(vault_usd_balance.u128()).is_equal_to(100_000u128);
    assert_that!(vault_wynd_balance.u128()).is_equal_to(100_000u128);

    Ok(())
}

// This test is going to be way easyer to setup if we have the option to deposit lp tokens.
#[test]
fn batch_unbond_pagination() -> anyhow::Result<()> {
    let owner = Addr::unchecked(common::OWNER);

    let (_, mock) = instantiate_default_mock_env(&owner)?;

    let mut vault = crate::create_vault(mock.clone())?;
    let mut vault_token = vault.vault_token.to_owned();
    let WynDex { .. } = vault.wyndex;

    // deposit big amount by owner:
    mock.set_balance(&owner, vec![coin(100_000u128, EUR), coin(100_000u128, USD)])?;
    vault.auto_compounder.deposit(
        vec![
            AnsAsset::new(AssetEntry::new("eur"), 100_000u128),
            AnsAsset::new(AssetEntry::new("usd"), 100_000u128),
        ],
        &[coin(100_000u128, EUR), coin(100_000u128, USD)],
    )?;

    let fake_addresses = (0..100)
        .map(|i| Addr::unchecked(format!("addr{i:}")))
        .collect::<Vec<Addr>>();
    fake_addresses.iter().for_each(|addr| {
        mock.set_balance(addr, vec![coin(10u128, EUR), coin(10u128, USD)])
            .unwrap();
    });

    // deposit 10 EUR for each address
    for addr in fake_addresses.iter() {
        vault.auto_compounder.set_sender(addr);
        vault.auto_compounder.deposit(
            vec![
                AnsAsset::new(AssetEntry::new("eur"), 10u128),
                AnsAsset::new(AssetEntry::new("usd"), 10u128),
            ],
            &[coin(10u128, EUR), coin(10u128, USD)],
        )?;
    }

    for addr in fake_addresses.iter() {
        let vault_token_balance = vault_token.balance(addr.to_string())?.balance;
        vault_token.set_sender(addr);
        vault_token.send(
            vault_token_balance,
            vault.auto_compounder.address()?.to_string(),
            to_binary(&Cw20HookMsg::Redeem {})?,
        )?;
    }
    // max 20 page per call. Test it by doing 30
    let claims = vault.auto_compounder.all_pending_claims(Some(30), None)?;
    assert_that!(claims.len()).is_equal_to(20);
    // loop over all pages of the all_pending_claims and concat to one vector
    drop(vault_token);

    let pending_claims = paginate_all_pending_claims(&vault)?;
    assert_that!(pending_claims.len()).is_equal_to(100);

    let claims = vault.auto_compounder.all_claims(None, None)?;
    assert_that!(claims.len()).is_equal_to(0);

    let _res = vault.auto_compounder.batch_unbond(Some(60), None)?;

    let all_claims = paginate_all_claims(&vault)?;
    assert_that!(all_claims.len()).is_equal_to(60);

    // default batch size is 100 so this should unbond the remaining 40
    let res = vault.auto_compounder.batch_unbond(None, None);
    assert_that!(res).is_ok();

    let all_claims = paginate_all_claims(&vault)?;
    assert_that!(all_claims.len()).is_equal_to(100);

    Ok(())
}

#[test]
fn test_lp_deposit() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let _user1: Addr = Addr::unchecked(common::USER1);
    let _commission_addr = Addr::unchecked(COMMISSION_RECEIVER);
    let _wyndex_owner = Addr::unchecked(WYNDEX_OWNER);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner).unwrap();

    // create a vault
    let vault = crate::create_vault(mock.clone()).unwrap();
    let WynDex {
        eur_usd_pair,
        eur_usd_lp,
        ..
    } = vault.wyndex;

    let vault_token = vault.vault_token;
    let _auto_compounder_addr = vault.auto_compounder.addr_str().unwrap();
    let _eur_asset = AssetEntry::new("eur");
    let _usd_asset = AssetEntry::new("usd");

    // check config setup
    let config: Config = vault.auto_compounder.config().unwrap();
    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address().unwrap());

    // give the user some lp tokens
    eur_usd_lp
        .call_as(&eur_usd_pair)
        .mint(100_000u128.into(), owner.to_string())?;

    // query how much lp tokens are in the vault
    let vault_lp_balance = vault.auto_compounder.total_lp_position().unwrap() as Uint128;
    assert_that!(vault_lp_balance.u128()).is_equal_to(0u128);

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(owner.to_string())?.balance;
    assert_that!(vault_token_balance.u128()).is_equal_to(0u128);

    // Deposit lps into the vault by owner
    eur_usd_lp.call_as(&owner).send(
        100_000u128.into(),
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::DepositLp {})?,
    )?;

    assert_that!(vault.auto_compounder.total_lp_position().unwrap().u128())
        .is_equal_to(100_000u128);
    assert_that!(vault_token.balance(owner.to_string())?.balance.u128())
        .is_equal_to(100_000u128 * 10u128.pow(DECIMAL_OFFSET));

    Ok(())
}

fn paginate_all_claims(vault: &Vault<Mock>) -> Result<Vec<(String, Vec<Claim>)>, anyhow::Error> {
    let mut all_claims = vec![];
    let mut start_after: Option<String> = None;
    loop {
        let claims = vault.auto_compounder.all_claims(Some(20), start_after)?;
        if claims.len() == 0 {
            break;
        }
        all_claims.extend(claims);
        start_after = Some(all_claims.last().unwrap().0.clone());
    }
    Ok(all_claims)
}

fn paginate_all_pending_claims(
    vault: &Vault<Mock>,
) -> Result<Vec<(String, Uint128)>, anyhow::Error> {
    let mut pending_claims: Vec<(String, Uint128)> = vec![];
    let mut start_after: Option<String> = None;
    loop {
        let claims = vault
            .auto_compounder
            .all_pending_claims(Some(30), start_after)?;
        if claims.len() == 0 {
            break;
        }
        pending_claims.extend(claims);
        start_after = Some(pending_claims.last().unwrap().0.clone());
    }
    Ok(pending_claims)
}

#[test]
fn vault_token_inflation_attack_original() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let user1: Addr = Addr::unchecked(common::USER1);
    let attacker: Addr = Addr::unchecked(ATTACKER);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner).unwrap();

    // create a vault
    let vault = crate::create_vault(mock.clone()).unwrap();
    let WynDex {
        eur_usd_pair,
        eur_usd_lp,
        eur_usd_staking,
        ..
    } = vault.wyndex;

    let config: Config = vault.auto_compounder.config().unwrap();
    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address().unwrap());

    let unbonding_secs = match config.unbonding_period {
        Some(Duration::Time(secs)) => secs,
        _ => panic!("unbonding period not in seconds"),
    };

    let vault_token = vault.vault_token;
    let auto_compounder_addr = vault.auto_compounder.addr_str().unwrap();

    let user_deposit = 100_000u128;
    // mint lp tokens to the user and the attacker
    eur_usd_lp
        .call_as(&eur_usd_pair)
        .mint(50002u128.into(), attacker.to_string())?;

    eur_usd_lp
        .call_as(&eur_usd_pair)
        .mint(100000u128.into(), user1.to_string())?;

    // attacker makes initial deposit to vault pool
    eur_usd_lp.call_as(&attacker).send(
        1u128.into(),
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::DepositLp {})?,
    )?;

    // check the number of vault tokens the attacker has
    let attacker_vault_token_balance = vault_token.balance(attacker.to_string())?.balance;
    assert_that!(attacker_vault_token_balance.u128())
        .is_equal_to(1u128 * 10u128.pow(DECIMAL_OFFSET));

    // attacker makes donation to liquidity pool
    let attacker_donation = user_deposit / 2 + 1u128;
    eur_usd_lp.call_as(&attacker).send(
        attacker_donation.into(),
        eur_usd_staking.to_string(),
        to_binary(&ReceiveDelegationMsg::Delegate {
            unbonding_period: unbonding_secs,
            delegate_as: Some(vault.account.proxy.addr_str()?),
        })?,
    )?;

    let lp_staked = vault.auto_compounder.total_lp_position().unwrap() as Uint128;
    assert_that!(lp_staked.u128()).is_equal_to(attacker_donation + 1);

    // user deposits lps to vault
    eur_usd_lp.call_as(&user1).send(
        100000u128.into(),
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::DepositLp {})?,
    )?;

    // check the amount of lp tokens staked by the vault in total
    let total_lp_staked = vault.auto_compounder.total_lp_position().unwrap() as Uint128;
    assert_that!(total_lp_staked.u128()).is_equal_to(150002);

    // check the amount of vault token the user has
    let user1_vault_token_balance = vault_token.balance(user1.to_string())?.balance;
    // including virual assets and 0 dec.offset: 100000 * ( 1 + 1) / (50001 + 1) = 3.999 -> 3
    // including virual assets and 1 dec.offset: 100000 * ( 1 + 10) / (50001 + 1) = 39.99 -> 39
    assert_that!(user1_vault_token_balance.u128())
        .is_equal_to(3.99_f32.mul(10.0_f32.powf(DECIMAL_OFFSET as f32)) as u128);

    // attacker withdraws the initial deposit
    vault_token.call_as(&attacker).send(
        (1u128 * 10u128.pow(DECIMAL_OFFSET)).into(),
        auto_compounder_addr.clone(),
        to_binary(&Cw20HookMsg::Redeem {})?,
    )?;

    // attacker unbonds tokens
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(attacker.to_string())?;
    assert_that!(pending_claims.u128()).is_equal_to(1u128 * 10u128.pow(DECIMAL_OFFSET));
    mock.wait_blocks(1)?;
    vault.auto_compounder.batch_unbond(None, None)?;

    let claim: Vec<Claim> = vault.auto_compounder.claims(attacker.to_string())?;
    assert_that!(claim.first().unwrap().amount_of_lp_tokens_to_unbond.u128())
        .is_less_than_or_equal_to(attacker_donation);
    // attackers donation is higher than the amount it retreives from the attack!

    mock.wait_blocks(60 * 60 * 24 * 10)?;
    Ok(())
}
#[test]
fn vault_token_inflation_attack_full_dilute() -> AResult {
    let owner = Addr::unchecked(common::OWNER);
    let user1: Addr = Addr::unchecked(common::USER1);
    let attacker: Addr = Addr::unchecked(ATTACKER);

    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner).unwrap();

    // create a vault
    let vault = crate::create_vault(mock.clone()).unwrap();
    let WynDex {
        eur_usd_pair,
        eur_usd_lp,
        eur_usd_staking,
        ..
    } = vault.wyndex;

    let config: Config = vault.auto_compounder.config().unwrap();
    assert_that!(config.liquidity_token).is_equal_to(eur_usd_lp.address().unwrap());

    let unbonding_secs = match config.unbonding_period {
        Some(Duration::Time(secs)) => secs,
        _ => panic!("unbonding period not in seconds"),
    };

    let vault_token = vault.vault_token;
    let _auto_compounder_addr = vault.auto_compounder.addr_str().unwrap();

    let user_deposit = 100_000u128;
    let attacker_deposit = 1u128;
    let fully_dilute_donation =
        (10u128.pow(DECIMAL_OFFSET) * user_deposit - 1) * (attacker_deposit + 1) + 1;
    // mint lp tokens to the user and the attacker
    eur_usd_lp.call_as(&eur_usd_pair).mint(
        (fully_dilute_donation + attacker_deposit).into(),
        attacker.to_string(),
    )?;

    eur_usd_lp
        .call_as(&eur_usd_pair)
        .mint(100000u128.into(), user1.to_string())?;

    // attacker makes initial deposit to vault pool
    eur_usd_lp.call_as(&attacker).send(
        1u128.into(),
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::DepositLp {})?,
    )?;

    // check the number of vault tokens the attacker has
    let attacker_vault_token_balance = vault_token.balance(attacker.to_string())?.balance;
    assert_that!(attacker_vault_token_balance.u128()).is_equal_to(10u128);

    // attacker makes donation to liquidity pool
    let attacker_donation = fully_dilute_donation;
    eur_usd_lp.call_as(&attacker).send(
        attacker_donation.into(),
        eur_usd_staking.to_string(),
        to_binary(&ReceiveDelegationMsg::Delegate {
            unbonding_period: unbonding_secs,
            delegate_as: Some(vault.account.proxy.addr_str()?),
        })?,
    )?;

    let lp_staked = vault.auto_compounder.total_lp_position().unwrap() as Uint128;
    assert_that!(lp_staked.u128()).is_equal_to(attacker_donation + 1);

    // user deposits lps to vault
    let resp = eur_usd_lp.call_as(&user1).send(
        user_deposit.into(),
        vault.auto_compounder.address()?.to_string(),
        to_binary(&Cw20HookMsg::DepositLp {})?,
    );

    // this will mint a zero amount so it will fail
    assert_that!(resp).is_err();

    Ok(())
}

fn generator_with_rewards_test_rewards_distribution_with_multiple_users() -> AResult {
    // test multiple user deposits and withdrawals
    todo!()
}
