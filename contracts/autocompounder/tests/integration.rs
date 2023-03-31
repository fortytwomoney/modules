#[cfg(test)]
mod test_utils;

use abstract_boot::{Abstract, AbstractBootError, ManagerQueryFns};
use abstract_core::api::{BaseExecuteMsgFns, BaseQueryMsgFns};
use abstract_core::objects::{AnsAsset, AssetEntry};
use abstract_sdk::core as abstract_core;

use abstract_boot::boot_core::*;
use autocompounder::state::{Claim, Config};
use boot_cw_plus::Cw20;
use cosmwasm_std::{
    coin, coins, to_binary, Addr, Binary, Decimal, Empty, StdResult, Timestamp, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_asset::Asset;
use cw_multi_test::{App, ContractWrapper, Executor};
use cw_staking::CW_STAKING;
use cw_utils::Expiration;
use dex::msg::*;
use dex::EXCHANGE;
use forty_two::autocompounder::{
    AutocompounderExecuteMsgFns, AutocompounderQueryMsg, AutocompounderQueryMsgFns,
    BondingPeriodSelector,
};
use forty_two::autocompounder::{Cw20HookMsg, AUTOCOMPOUNDER};
use forty_two_boot::autocompounder::AutocompounderApp;
use speculoos::assert_that;
use speculoos::prelude::OrderedAssertions;
use test_utils::abstract_helper::{self, init_auto_compounder};
use test_utils::vault::Vault;
use test_utils::{AResult, DISTRIBUTION, OWNER};

use wyndex_bundle::*;

const WYNDEX: &str = "wyndex";
const COMMISSION_RECEIVER: &str = "commission_receiver";
const VAULT_TOKEN: &str = "vault_token";

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

    // Deploy mock dex
    let wyndex = WynDex::deploy_on(mock.clone(), Empty {})?;

    let eur_asset = AssetEntry::new(EUR);
    let usd_asset = AssetEntry::new(USD);

    // Set up the dex and staking contracts
    let exchange_api = abstract_helper::init_exchange(mock.clone(), &abstract_, None)?;
    let staking_api = abstract_helper::init_staking(mock.clone(), &abstract_, None)?;
    let auto_compounder = init_auto_compounder(mock.clone(), &abstract_, None)?;

    let mut vault_token = Cw20::new(VAULT_TOKEN, mock.clone());
    // upload the vault token code
    let vault_toke_code_id = vault_token.upload()?.uploaded_code_id()?;
    // Create an Account that we will turn into a vault
    let os = abstract_.account_factory.create_default_account(
        abstract_core::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;

    // install dex
    os.manager.install_module(EXCHANGE, &Empty {})?;
    // install staking
    os.manager.install_module(CW_STAKING, &Empty {})?;
    // install autocompounder
    os.manager.install_module(
        AUTOCOMPOUNDER,
        &abstract_core::app::InstantiateMsg {
            module: forty_two::autocompounder::AutocompounderInstantiateMsg {
                code_id: vault_toke_code_id,
                commission_addr: COMMISSION_RECEIVER.to_string(),
                deposit_fees: Decimal::percent(3),
                dex: WYNDEX.to_string(),
                fee_asset: eur_asset.to_string(),
                performance_fees: Decimal::percent(3),
                pool_assets: vec![eur_asset, usd_asset],
                withdrawal_fees: Decimal::percent(3),
                preferred_bonding_period: BondingPeriodSelector::Shortest,
            },
            base: abstract_core::app::BaseInstantiateMsg {
                ans_host_address: abstract_.ans_host.addr_str()?,
            },
        },
    )?;
    // get its address
    let auto_compounder_addr = os
        .manager
        .module_addresses(vec![AUTOCOMPOUNDER.into()])?
        .modules[0]
        .1
        .clone();
    // set the address on the contract
    auto_compounder.set_address(&Addr::unchecked(auto_compounder_addr.clone()));

    // give the autocompounder permissions to call on the dex and cw-staking contracts
    exchange_api
        .call_as(&os.manager.address()?)
        .update_traders(vec![auto_compounder_addr.clone()], vec![])?;
    staking_api
        .call_as(&os.manager.address()?)
        .update_traders(vec![auto_compounder_addr], vec![])?;

    // set the vault token address
    let auto_compounder_config = auto_compounder.config()?;
    vault_token.set_address(&auto_compounder_config.vault_token);

    Ok(Vault {
        os,
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
    let owner = Addr::unchecked(test_utils::OWNER);

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
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(10000u128);

    // and eur balance decreased and usd balance stayed the same
    let balances = mock.query_all_balances(&owner)?;

    // .sort_by(|a, b| a.denom.cmp(&b.denom));
    assert_that!(balances).is_equal_to(vec![
        coin(90_000u128, eur_token.to_string()),
        coin(90_000u128, usd_token.to_string()),
    ]);

    // withdraw part from the auto-compounder
    vault_token.send(&Cw20HookMsg::Redeem {}, 2000, auto_compounder_addr.clone())?;
    // check that the vault token decreased
    let vault_token_balance = vault_token.balance(&owner)?;
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(vault_token_balance).is_equal_to(8000u128);
    assert_that!(pending_claims.u128()).is_equal_to(2000u128);

    // check that the pending claims are updated
    vault_token.send(&Cw20HookMsg::Redeem {}, 2000, auto_compounder_addr.clone())?;
    let pending_claims: Uint128 = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(pending_claims.u128()).is_equal_to(4000u128);

    vault.auto_compounder.batch_unbond()?;

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
        .query_all_staked(asset_infos, &vault.os.proxy.addr_str()?)?;

    let generator_staked_balance = staked.stakes.first().unwrap();
    assert_that!(generator_staked_balance.stake.u128()).is_equal_to(6000u128);

    // withdraw all from the auto-compounder
    vault_token.send(&Cw20HookMsg::Redeem {}, 6000, auto_compounder_addr)?;
    vault.auto_compounder.batch_unbond()?;
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
#[test]
fn generator_without_reward_proxies_single_sided() -> AResult {
    let owner = Addr::unchecked(test_utils::OWNER);

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
    let config: Config = vault.auto_compounder.config()?;
    let position = vault.auto_compounder.total_lp_position()?;
    assert_that!(position).is_equal_to(Uint128::zero());

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
            AnsAsset::new(eur_asset.clone(), 10000u128),
            AnsAsset::new(usd_asset.clone(), 10000u128),
        ],
        &[coin(10_000u128, EUR), coin(10_000u128, USD)],
    )?;

    let position = vault.auto_compounder.total_lp_position()?;
    assert_that!(position).is_greater_than(Uint128::zero());

    // single asset deposit
    vault.auto_compounder.deposit(
        vec![AnsAsset::new(eur_asset, 1000u128)],
        &[coin(1000u128, EUR)],
    )?;

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(10487u128);
    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_greater_than(position);

    vault.auto_compounder.deposit(
        vec![AnsAsset::new(usd_asset, 1000u128)],
        &[coin(1000u128, USD)],
    )?;

    // check that the vault token is increased
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(10986u128);
    // check if the vault balance query functions properly:
    let vault_balance_queried = vault.auto_compounder.balance(owner.to_string())?;
    assert_that!(vault_balance_queried).is_equal_to(Uint128::from(vault_token_balance));

    let position = new_position;
    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_greater_than(position);

    // and eur balance decreased and usd balance stayed the same
    let balances = mock.query_all_balances(&owner)?;
    assert_that!(balances).is_equal_to(vec![
        coin(89_000u128, eur_token.to_string()),
        coin(89_000u128, usd_token.to_string()),
    ]);

    // withdraw part from the auto-compounder
    vault_token.send(&Cw20HookMsg::Redeem {}, 4986, auto_compounder_addr.clone())?;
    // check that the vault token decreased
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(6000u128);

    let pending_claim = vault.auto_compounder.pending_claims(owner.to_string())?;
    assert_that!(pending_claim.u128()).is_equal_to(4986u128);
    let vault_token_balance = vault_token.balance(&vault.auto_compounder.address()?)?;
    assert_that!(vault_token_balance).is_equal_to(4986u128);

    let total_lp_balance = vault.auto_compounder.total_lp_position()?;
    assert_that!(total_lp_balance).is_equal_to(new_position);

    // Batch unbond pending claims
    vault.auto_compounder.batch_unbond()?;

    // query the claims of the auto-compounder
    let claims = vault.auto_compounder.claims(owner.to_string())?;
    let expected_claim = Claim {
        unbonding_timestamp: Expiration::AtTime(mock.block_info()?.time.plus_seconds(1)),
        amount_of_vault_tokens_to_burn: 4986u128.into(),
        amount_of_lp_tokens_to_unbond: 4985u128.into(),
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
        coin(93_988u128, eur_token.to_string()),
        coin(93_988u128, usd_token.to_string()),
    ]);

    let position = new_position;
    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_less_than(position);

    let generator_staked_balance = vault
        .wyndex
        .suite
        .query_all_staked(asset_infos, &vault.os.proxy.addr_str()?)?
        .stakes[0]
        .stake;
    assert_that!(generator_staked_balance.u128()).is_equal_to(6001u128);

    // withdraw all from the auto-compounder
    vault_token.send(&Cw20HookMsg::Redeem {}, 6000, auto_compounder_addr)?;

    // testing general non unbonding staking contract functionality
    let pending_claims = vault
        .auto_compounder
        .pending_claims(owner.to_string())?
        .into();
    assert_that!(pending_claims).is_equal_to(6000u128); // no unbonding period, so no pending claims

    vault.auto_compounder.batch_unbond()?; // batch unbonding not enabled
    mock.wait_blocks(60 * 60 * 24 * 10)?;
    vault.auto_compounder.withdraw()?; // withdraw wont have any effect, because there are no pending claims
                                       // mock.next_block()?;

    let balances = mock.query_all_balances(&owner)?;
    assert_that!(balances).is_equal_to(vec![
        coin(99_993u128, eur_token.to_string()),
        coin(99_993u128, usd_token.to_string()),
    ]);

    let new_position = vault.auto_compounder.total_lp_position()?;
    assert_that!(new_position).is_equal_to(Uint128::zero());

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
    let owner = Addr::unchecked(test_utils::OWNER);
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
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(100_000u128);
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
        &Cw20HookMsg::Redeem {},
        vault_token_balance,
        auto_compounder_addr,
    )?;

    // Unbond tokens & clear pending claims
    vault.auto_compounder.batch_unbond()?;

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

fn generator_with_rewards_test_rewards_distribution_with_multiple_users() -> AResult {
    // test multiple user deposits and withdrawals
    todo!()
}
