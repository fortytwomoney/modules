use std::str::FromStr;

use crate::common::dexes::OSMOSIS_EPOCH;

use super::dexes::DexInit;
use abstract_testing::addresses::EUR;
use autocompounder::msg::Claim;
use autocompounder::msg::FeeConfig;
use autocompounder::state::DECIMAL_OFFSET;

use autocompounder::state::Config;
use cosmwasm_std::Decimal;
use cw_orch::prelude::*;
use speculoos::vec::VecAssertions;

use super::vault::GenericVault;
use super::AResult;
#[allow(unused_imports)]
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};
use cosmwasm_std::{Addr, Uint128};

use speculoos::{assert_that, numeric::OrderedAssertions};


#[allow(dead_code)]
pub fn test_deposit_assets<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    user1: &<Chain as TxHandler>::Sender,
    user1_addr: &Addr,
    user2: &<Chain as TxHandler>::Sender,
    user2_addr: &Addr,
) -> AResult {
    let _ac_addres = vault.autocompounder_app.addr_str()?;
    let _config: Config = vault.autocompounder_app.config()?;
    let amount = 10_000u128;

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user1, amount, amount, None)?;
    let mut vt_total_supply = vault.assert_expected_shares(0u128, 0u128, 0u128, user1_addr)?;

    // deposit `amount` of only the first asset and check whether the vault tokens of u1 have increased
    let prev_lp_amount = vault.total_lp_position()?;
    let prev_vt_balance = vault.vault_token_balance(user1_addr)?;
    vault.deposit_assets(user1, amount, 0, None)?;
    vt_total_supply += vault.assert_expected_shares(prev_lp_amount, vt_total_supply, prev_vt_balance, user1_addr)?;

    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    let prev_lp_amount = vault.total_lp_position()?;
    let prev_vt_balance = vault.vault_token_balance(user2_addr)?;
    vault.deposit_assets(user2, amount, amount, None)?;
    vault.assert_expected_shares(prev_lp_amount, vt_total_supply, prev_vt_balance, user2_addr)?;

    Ok(())
}

/// Deposit assets with a recipient
/// Note: im not exactly sure whether the balances returned should be exactly equal, like the test implies.
#[allow(dead_code)]
pub fn deposit_with_recipient<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    user1: &<Chain as TxHandler>::Sender,
    user1_addr: &Addr,
    user2: &<Chain as TxHandler>::Sender,
    user2_addr: &Addr,
) -> AResult {
    let _ac_address = vault.autocompounder_app.addr_str()?;
    let _config: Config = vault.autocompounder_app.config()?;
    let amount = 10_000u128;

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user1, amount, amount, Some(user2_addr.clone()))?;
    let mut vt_total_supply = vault.assert_expected_shares(0u128, 0u128, 0u128, user2_addr)?;

    // deposit `amount` of only the first asset and check whether the vault tokens of u2 have increased

    let prev_lp_amount = vault.total_lp_position()?;
    let prev_vt_balance = vault.vault_token_balance(user2_addr)?; 
    vault.deposit_assets(user1, amount, 0, Some(user2_addr.clone()))?;
    vt_total_supply += vault.assert_expected_shares(prev_lp_amount, vt_total_supply, prev_vt_balance, user2_addr)?;

    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    let prev_lp_amount = vault.total_lp_position()?;
    let prev_u2_vt_balance = vault.vault_token_balance(user2_addr)?;
    vault.deposit_assets(user2, amount, amount, Some(user1_addr.clone()))?;
    vault.assert_expected_shares(prev_lp_amount, vt_total_supply, 0u128, user1_addr)?;
    // also check that the vault token balance of user2 has not changed
    assert_that!(vault.vault_token_balance(user2_addr)?).is_equal_to(prev_u2_vt_balance);

    Ok(())
}

#[allow(dead_code)]
pub fn redeem_deposit_immediately_with_unbonding<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    user1: &<Chain as TxHandler>::Sender,
    user1_addr: &Addr,
    user2: &<Chain as TxHandler>::Sender,
    user2_addr: &Addr,
) -> AResult {
    let _ac_address = vault.autocompounder_app.addr_str()?;
    let config: Config = vault.autocompounder_app.config()?;
    let amount = 10_000u128;
    require_unbonding_period(config, true);

    let u1_init_balances= vault.pool_assets_balances(user1_addr.to_string())?;

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user1, amount, amount, None)?;
    let vt_total_supply = vault.assert_expected_shares(0u128, 0u128, 0u128, user1_addr)?;

    // redeem assets and check whether the vault tokens of u1 have decreased
    let u1_vt_balance = vt_total_supply;
    let prev_lp_amount = vault.total_lp_position()?;
    let redeem_amount = u1_vt_balance.div_ceil(4u128);
    vault.redeem_vault_token(redeem_amount, user1, None)?;
    vault.assert_redeem_before_unbonding(user1_addr, prev_lp_amount, u1_vt_balance, redeem_amount, 0u128,None)?;

    // redeem the rest
    assert_that!(u1_vt_balance).is_greater_than_or_equal_to(redeem_amount);
    let rest_redeem_amount = u1_vt_balance - redeem_amount;
    let total_redeem_amount = u1_vt_balance;
    vault.redeem_vault_token(rest_redeem_amount, user1, None)?;
    vault.assert_redeem_before_unbonding(user1_addr, prev_lp_amount, rest_redeem_amount, rest_redeem_amount, redeem_amount,  None)?;


    // call batch unbond and progress block time by one second
    vault.assert_batch_unbond(prev_lp_amount, total_redeem_amount)?;
    let claims: Vec<Claim> = vault.autocompounder_app.claims(user1_addr.clone())?;
    assert_that!(claims).has_length(1);

    println!("{:?}", claims.first().unwrap());
    vault.chain.wait_seconds(1).unwrap();


    // Withdraw the pending claims and check whether the vault tokens of u1 have increased
    vault.withdraw_and_assert(user1, user1_addr, u1_init_balances)?;

    // Same test as above but with recipient
    let (u2_a_initial_balance, u2_b_initial_balance) = vault.pool_assets_balances(user2_addr)?;

    vault.deposit_assets(user1, amount, amount, None)?;
    let u1_vt_balance = vault.assert_expected_shares(0u128, 0u128, 0u128, user1_addr)?;

    vault.redeem_vault_token(u1_vt_balance, user1, Some(user2_addr.clone()))?;
    let prev_lp_amount = vault.total_lp_position()?;
    vault.assert_redeem_before_unbonding(user2_addr, prev_lp_amount, u1_vt_balance, rest_redeem_amount, 0u128, None)?;
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_new_vt_balance).is_equal_to(0u128);

    vault.assert_batch_unbond(prev_lp_amount, u1_vt_balance)?;
    vault.chain.wait_seconds(1).unwrap();


    vault.withdraw_and_assert(user2, user2_addr, (u2_a_initial_balance+amount, u2_b_initial_balance + amount))?;
    Ok(())
}



/// Deposit fees, fee token handling, and withdrawal fees test
#[allow(dead_code)]
pub fn deposit_fees_fee_token_and_withdraw_fees<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    owner: &<Chain as TxHandler>::Sender,
    owner_addr: &Addr,
    commission_addr: &Addr,
) -> AResult {
    let amount = 1_000u128;
    let deposit_fee_percentage = Decimal::from_str("0.01")?;
    let withdrawal_fee_percentage = Decimal::from_str("0.1")?;
    let performance_fee_percentage = Decimal::zero();

    // Update performance fees to zero and deposit/withdrawal fees to 10%
    vault.autocompounder_app.update_fee_config(
        Some(performance_fee_percentage),
        Some(deposit_fee_percentage.to_string()),
        Some(withdrawal_fee_percentage),
        None,
    )?;

    let fee_config: FeeConfig = vault.autocompounder_app.fee_config()?;
    assert_that!(fee_config.deposit).is_equal_to(deposit_fee_percentage);
    assert_that!(fee_config.withdrawal).is_equal_to(withdrawal_fee_percentage);
    assert_that!(fee_config.performance).is_equal_to(performance_fee_percentage);

    // Deposit 1000 EUR and check fees
    vault.deposit_assets(owner, amount, 0, None)?;
    vault.assert_expected_shares(0u128, 0u128, 0u128, owner_addr)?;
    let expected_deposit_fee = Uint128::from(amount) * deposit_fee_percentage;

    let received_fee = vault.chain.bank_querier().balance(commission_addr, Some(EUR.to_string())).unwrap().first().unwrap().amount;
    assert_that!(received_fee).is_equal_to(expected_deposit_fee);

    // Check the owner's vault token balance after deposit
    let owner_vault_balance = vault.vault_token_balance(owner_addr)?;
    assert_that!(owner_vault_balance).is_greater_than(0u128);

    // Redeem all and check withdrawal fees
    let redeem_amount = owner_vault_balance;
    let prev_lp_amount = vault.total_lp_position()?;
    vault.redeem_vault_token(redeem_amount, owner, None)?;
    assert_that!(vault.pending_claims(owner_addr)?).is_equal_to(redeem_amount);


    vault.assert_batch_unbond(prev_lp_amount, redeem_amount)?;

    // Check pending claims which should consider withdrawal fees
    let claims: Vec<Claim> = vault.autocompounder_app.claims(owner_addr.clone())?;
    let expected_withdrawal_fee = (Uint128::from(redeem_amount) * withdrawal_fee_percentage).u128();
    assert_that!(claims.first().unwrap().amount_of_vault_tokens_to_burn.u128()).is_equal_to(redeem_amount - expected_withdrawal_fee);

    assert_that!(vault.total_supply()?).is_equal_to(0u128);

    Ok(())
}

#[allow(dead_code)]
pub fn compound_reward_distribution<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    owner: &<Chain as TxHandler>::Sender,
    owner_addr: &Addr,
    commission_addr: &Addr,
) -> AResult {
    let amount = 100_000u128;
    let reward_amount = 1_000u128;
    let reward_token = vault.reward_token();

    // Initial deposit
    vault.deposit_assets(owner, amount, amount, None)?;

    // Query LP tokens in the vault before reward distribution
    let vault_lp_balance_before_rewards = vault.total_lp_position()?;

    // Simulate block pass and distribute rewards
    // NOTE: For osmosis we need to wait one epoch for the rewards to be distributed
    // TODO: This should be handled by the dex/vault
    vault.chain.wait_seconds(OSMOSIS_EPOCH).unwrap();

    // Compound rewards
    vault.autocompounder_app.compound()?;

    // Assert fee distribution is correct
    let commission_received = vault.asset_balance(commission_addr, &reward_token)?;
    assert_that!(commission_received).is_equal_to(30u128);

    // Query LP tokens in the vault after reward distribution
    let vault_lp_balance_after_rewards = vault.total_lp_position()?;
    let new_lp_tokens = vault_lp_balance_after_rewards - vault_lp_balance_before_rewards;

    // Expected new LP value, assuming a 0.4% increase due to rewards
    let expected_new_lp_tokens = vault_lp_balance_before_rewards * 4u128 / 1000u128;
    assert!(new_lp_tokens > expected_new_lp_tokens, "New LP tokens are less than expected");

    // Redeem vault tokens to check if rewards were correctly allocated
    let owner_vt_balance = vault.vault_token_balance(owner_addr)?;
    vault.redeem_vault_token(owner_vt_balance, owner, None)?;

    // // Assert user has more EUR and USD than they deposited
    // let (eur_diff, usd_diff) = vault.assert_asset_difference(owner_addr, &(eur_token, usd_token), amount)?;
    // assert!(eur_diff > 0, "EUR balance did not increase as expected");
    // assert!(usd_diff > 0, "USD balance did not increase as expected");

    Ok(())
}

#[allow(dead_code)]
pub fn vault_token_inflation_attack<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    owner: &<Chain as TxHandler>::Sender,
    owner_addr: &Addr,
    attacker: &<Chain as TxHandler>::Sender,
    attacker_addr: &Addr,
    minter: &<Chain as TxHandler>::Sender,
    minter_addr: &Addr,
) -> AResult {
    let user_deposit = 100_000u128;
    let attacker_initial_deposit = 1u128;
    let attacker_donation = user_deposit / 2 + 1u128;

    // Mint LP tokens to the user and the attacker
    vault.mint_lp_tokens(minter, minter_addr, vec![owner_addr, attacker_addr], user_deposit)?;

    // Attacker makes initial deposit to vault pool
    vault.deposit_lp_token(attacker, attacker_initial_deposit, None)?;

    // Check the number of vault tokens the attacker has
    let attacker_vault_token_balance = vault.vault_token_balance(attacker_addr)?;
    assert_eq!(attacker_vault_token_balance, 10u128.pow(DECIMAL_OFFSET), "Attacker vault token balance mismatch");

    // Attacker makes a donation to the liquidity pool
    vault.donate_lp_to_liquidity_pool(attacker, attacker_donation)?;

    // Check the total LP staked after donation
    let lp_staked_after_donation = vault.total_lp_position()?;
    assert_eq!(lp_staked_after_donation, attacker_donation + attacker_initial_deposit, "LP staked after donation mismatch");

    // User deposits LP tokens into the vault
    vault.deposit_lp_token(owner, user_deposit, None)?;

    // Check the total LP tokens staked by the vault
    let total_lp_staked = vault.total_lp_position()?;
    assert_eq!(total_lp_staked, 150002u128, "Total LP staked mismatch");

    // Check the amount of vault token the user has
    let user_vault_token_balance = vault.vault_token_balance(owner_addr)?;
    let attacker_vault_token_balance = vault.vault_token_balance(attacker_addr)?;
    // The calculation for user_vault_token_balance should consider the virtual assets and rounding logic used by the vault
    // This will need to be adjusted based on the vault's specific logic for token minting

    // TODO: Check balance user and attacker
    assert_that!(user_vault_token_balance).is_greater_than_or_equal_to(attacker_vault_token_balance);

    Ok(())
}

#[allow(dead_code)]
pub fn vault_token_inflation_attack_full_dilute<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    owner: &<Chain as TxHandler>::Sender,
    owner_addr: &Addr,
    attacker: &<Chain as TxHandler>::Sender,
    attacker_addr: &Addr,
    minter: &<Chain as TxHandler>::Sender,
    minter_addr: &Addr,
) -> AResult {
    let user_deposit = 100_000u128;
    let attacker_deposit = 1u128;
    // Calculate the fully dilute donation amount based on the formula provided in the original test
    let fully_dilute_donation = (10u128.pow(DECIMAL_OFFSET) * user_deposit - 1) * (attacker_deposit + 1) + 1;

    vault.mint_lp_tokens(minter, minter_addr, vec![owner_addr, attacker_addr], user_deposit+attacker_deposit)?;
    // Mint LP tokens to the user and the attacker

    // Attacker makes initial deposit to vault pool
    vault.deposit_lp_token(attacker, attacker_deposit, None)?;

    // Check the number of vault tokens the attacker has
    let attacker_vault_token_balance = vault.vault_token_balance(attacker_addr)?;
    assert_eq!(attacker_vault_token_balance, 10u128, "Attacker vault token balance mismatch");

    // Attacker makes a donation to the liquidity pool
    vault.donate_lp_to_liquidity_pool(attacker, fully_dilute_donation)?;

    // Check the total LP staked after the attacker's donation
    let lp_staked_after_donation = vault.total_lp_position()?;
    assert_eq!(lp_staked_after_donation, fully_dilute_donation + attacker_deposit, "LP staked after donation mismatch");

    // User attempts to deposit LP tokens into the vault
    let deposit_result = vault.deposit_lp_token(owner, user_deposit, None);

    // Assert that the deposit attempt fails due to the dilution effect caused by the attacker's actions
    assert!(deposit_result.is_err(), "Deposit did not fail as expected due to full dilution");

    Ok(())
}


/// Redeem vault tokens for vaoult without unbonding period.
#[allow(dead_code)]
pub fn redeem_deposit_immediately_without_unbonding<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    _user1: &<Chain as TxHandler>::Sender,
    _user1_addr: &Addr,
    _user2: &<Chain as TxHandler>::Sender,
    _user2_addr: &Addr,
) -> AResult {
    let _ac_address = vault.autocompounder_app.addr_str()?;
    let config: Config = vault.autocompounder_app.config()?;
    let _amount = 10_000u128;
    require_unbonding_period(config, false);

    todo!("implement this test once we have a testsuite that has nonbonding periods")
}


fn require_unbonding_period(config: Config, state: bool) {
    if state && config.unbonding_period.is_none() {
        panic!("This test requires an unbonding period to be set in the config. Either remove this test or make sure its setup correctly.");
    }
    if !state && config.unbonding_period.is_some() {
        panic!("This test requires an unbonding period to be not set in the config. Either remove this test or make sure its setup correctly.");
    }
}
