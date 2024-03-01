use super::dexes::DexInit;
use autocompounder::state::DECIMAL_OFFSET;

use autocompounder::state::Config;
use cw_orch::prelude::*;
use speculoos::result::ResultAssertions;

use super::vault::GenericVault;
use super::AResult;
#[allow(unused_imports)]
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};
use cosmwasm_std::{Addr, Uint128};

use speculoos::{assert_that, numeric::OrderedAssertions};

pub fn convert_to_shares(assets: Uint128, total_assets: Uint128, total_supply: Uint128) -> Uint128 {
    assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(DECIMAL_OFFSET),
        total_assets + Uint128::from(1u128),
    )
}

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

    let u1_lp_token_amount: u128 = vault.autocompounder_app.total_lp_position()?.into();
    let u1_vt_balance = vault.vault_token_balance(user1_addr)?;
    let mut vt_total_supply = u1_vt_balance; // only depositor for now
    assert_that!(u1_vt_balance).is_equal_to(u1_lp_token_amount * 10u128.pow(DECIMAL_OFFSET));

    // deposit `amount` of only the first asset and check whether the vault tokens of u1 have increased
    vault.deposit_assets(user1, amount, 0, None)?;

    let u1_new_lp_token_amount: u128 = vault.autocompounder_app.total_lp_position()?.into();
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr)?;
    assert_that!(u1_new_lp_token_amount - u1_lp_token_amount).is_greater_than(0u128);
    assert_that!(u1_new_vt_balance - u1_vt_balance).is_greater_than(0u128);
    

    let u1_added_lp_token_amount = u1_new_lp_token_amount - u1_lp_token_amount;
    let u1_gained_vaulttoken = u1_new_vt_balance - u1_vt_balance;

    // check the new vault token amount
    let expected_mint_amount = convert_to_shares(u1_added_lp_token_amount.into(), u1_lp_token_amount.into(),vt_total_supply.into()).u128();
    assert_that!(u1_gained_vaulttoken).is_equal_to(expected_mint_amount);
    vt_total_supply += u1_gained_vaulttoken;

    // TODO: check whether the balance is exactly what it should be here

    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user2, amount, amount, None)?;
    let u2_added_lp_amount =
        vault.autocompounder_app.total_lp_position()?.u128() - u1_new_lp_token_amount;
    let u2_vt_balance = vault.vault_token_balance(user2_addr)?;

    // check the new vault token amount
    let expected_mint_amount = convert_to_shares(u2_added_lp_amount.into(), u1_new_lp_token_amount.into(),vt_total_supply.into()).u128();
    assert_that!(u2_vt_balance).is_equal_to(expected_mint_amount);

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

    let u1_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u2_vt_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u2_vt_balance).is_equal_to(u1_lp_token_amount.u128() * 10u128.pow(DECIMAL_OFFSET));

    // deposit `amount` of only the first asset and check whether the vault tokens of u2 have increased
    vault.deposit_assets(user1, amount, 0, Some(user2_addr.clone()))?;

    let u1_new_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u2_new_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u1_new_lp_token_amount - u1_lp_token_amount).is_greater_than(Uint128::zero());
    assert_that!(u2_new_balance - u2_vt_balance).is_greater_than(0u128);



    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user2, amount, amount, Some(user1_addr.clone()))?;
    let u2_lp_token_amount: Uint128 =
        vault.autocompounder_app.total_lp_position()? - u1_new_lp_token_amount;
    let u1_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u2_lp_token_amount).is_equal_to(u1_lp_token_amount);
    assert_that!(u1_balance).is_equal_to(u2_vt_balance);

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

    let (u1_a_initial_balance, u1_b_initial_balance) = vault.asset_balances(user1_addr.to_string())?;

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user1, amount, amount, None)?;

    let u1_lp_token_amount: u128 = vault.autocompounder_app.total_lp_position()?.into();
    let u1_vt_balance = vault.vault_token_balance(user1_addr)?;
    assert_that!(u1_vt_balance).is_equal_to(u1_lp_token_amount * 10u128.pow(DECIMAL_OFFSET));

    // redeem assets and check whether the vault tokens of u1 have decreased
    vault.redeem_vault_token(u1_vt_balance, user1, None)?;
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr)?;
    assert_that!(u1_new_vt_balance).is_equal_to(0u128);

    let u1_pending_claims = vault.pending_claims(user1_addr)?;
    assert_that!(u1_pending_claims).is_equal_to(u1_vt_balance);

    let u1_new_lp_token_amount: u128 = vault.autocompounder_app.total_lp_position()?.into();
    assert_that!(u1_new_lp_token_amount).is_equal_to(u1_lp_token_amount);

    // call batch unbond and progress block time by one second
    vault.autocompounder_app.batch_unbond(None, None)?;
    let ac_vt_balance = vault.vault_token_balance(_ac_address.clone())?;
    assert_that!(ac_vt_balance).is_equal_to(0u128);

    // TODO: optionally check whether the locked tokens are now in the unbonding queue
    // this is quite hard to generallize and might not be necessary if the rest works

    vault.chain.next_block().unwrap();

    // Withdraw the pending claims and check whether the vault tokens of u1 have increased
    vault.autocompounder_app.call_as(user1).withdraw()?;
    let (u1_a_balance, u1_b_balance) = vault.asset_balances(user1_addr)?;
    assert_that!(u1_a_balance).is_equal_to(amount);
    assert_that!(u1_b_balance).is_equal_to(amount);

    // Same test as above but with recipient
    let (u2_a_initial_balance, u2_b_initial_balance) = vault.asset_balances(user2_addr)?;

    vault.deposit_assets(user1, amount, amount, Some(user2_addr.clone()))?;
    let u1_vt_balance = vault.vault_token_balance(user1_addr)?;

    vault.redeem_vault_token(u1_vt_balance, user1, Some(user2_addr.clone()))?;
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_new_vt_balance).is_equal_to(0u128);
    let u1_pending_claims: u128 = vault
        .autocompounder_app
        .pending_claims(user1_addr.clone())?
        .into();
    assert_that!(u1_pending_claims).is_equal_to(0u128);

    let (u1_a_balance, u1_b_balance) = vault.asset_balances(user1_addr)?;
    let (u2_a_balance, u2_b_balance) = vault.asset_balances(user2_addr)?;
    assert_that!(u1_a_balance).is_equal_to(u1_a_initial_balance - amount);
    assert_that!(u1_b_balance).is_equal_to(u1_b_initial_balance - amount);
    assert_that!(u2_a_balance).is_equal_to(u2_a_initial_balance + amount);
    assert_that!(u2_b_balance).is_equal_to(u2_b_initial_balance + amount);

    Ok(())
}

/// Redeem vault tokens without unbonding.
/// tests both with and without recipient.
#[allow(dead_code)]
pub fn redeem_deposit_immediately_without_unbonding<Chain: CwEnv, Dex: DexInit>(
    vault: GenericVault<Chain, Dex>,
    user1: &<Chain as TxHandler>::Sender,
    user1_addr: &Addr,
    user2: &<Chain as TxHandler>::Sender,
    user2_addr: &Addr,
) -> AResult {
    let _ac_address = vault.autocompounder_app.addr_str()?;
    let config: Config = vault.autocompounder_app.config()?;
    let amount = 10_000u128;
    require_unbonding_period(config, false);

    let (u1_a_init_balance, u1_b_init_balance) = vault.asset_balances(user1_addr)?;

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user1, amount, amount, None)?;
    let u1_vt_balance = vault.vault_token_balance(user1_addr)?;

    vault.redeem_vault_token(u1_vt_balance, user1, None)?;
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr)?;
    assert_that!(u1_new_vt_balance).is_equal_to(0u128);
    let u1_pending_claims= vault.pending_claims(user1_addr)?;
    assert_that!(u1_pending_claims).is_equal_to(0u128);

    let (u1_a_balance, u1_b_balance) = vault.asset_balances(user1_addr)?;
    assert_that!(u1_a_balance).is_equal_to(u1_a_init_balance);
    assert_that!(u1_b_balance).is_equal_to(u1_b_init_balance);
    
    
    // Same test as above but with recipient
    let (u2_a_initial_balance, u2_b_initial_balance) = vault.asset_balances(user2_addr)?;
    vault.deposit_assets(user1, amount, amount, Some(user2_addr.clone()))?;
    let u1_vt_balance = vault.vault_token_balance(user1_addr)?;
    
    vault.redeem_vault_token(u1_vt_balance, user1, Some(user2_addr.clone()))?;
    let u1_new_vt_balance = vault.vault_token_balance(user1_addr)?;
    assert_that!(u1_new_vt_balance).is_equal_to(0u128);
    let u1_pending_claims= vault.pending_claims(user1_addr)?;
    assert_that!(u1_pending_claims).is_equal_to(0u128);

    let (u1_a_balance, u1_b_balance) = vault.asset_balances(user1_addr)?;
    let (u2_a_balance, u2_b_balance) = vault.asset_balances(user2_addr)?;
    assert_that!(u1_a_balance).is_equal_to(0u128);
    assert_that!(u1_b_balance).is_equal_to(0u128);
    assert_that!(u2_a_balance).is_equal_to(u2_a_initial_balance + amount);
    assert_that!(u2_b_balance).is_equal_to(u2_b_initial_balance + amount);

    Ok(())
}

// // TOFIX: need some detailse fixed
// pub fn vault_token_inflation_attack_original<Chain: CwEnv, Dex: DexInit>(
//     vault: GenericVault<Chain, Dex>,
//     user: &<Chain as TxHandler>::Sender,
//     user_addr: &Addr,
//     attacker: &<Chain as TxHandler>::Sender,
//     attacker_addr: &Addr,
// ) -> AResult {
//     let _ac_address = vault.autocompounder_app.addr_str()?;
//     let config: Config = vault.autocompounder_app.config()?;
//     let amount = 100_000u128; // Adjusted to match the old test's user_deposit
//     require_unbonding_period(config, false); // Assuming no unbonding is required for this scenario, adjust based on actual needs

//     // Attacker makes initial deposit to vault pool
//     vault.deposit_assets(attacker, amount / 2, 0, None)?;
//     let attacker_initial_vt_balance = vault.vault_token_balance(attacker_addr)?;

//     // Attacker makes donation to liquidity pool
//     // Note: Assuming `donate_to_liquidity_pool` is a method available in the new framework to mimic the donation behavior
//     donate_to_liquidity_pool(&vault, attacker, amount / 2 + 1)?;

//     let lp_staked_after_donation: u128 = vault.autocompounder_app.total_lp_position()?.into();
//     assert_that!(lp_staked_after_donation).is_equal_to(amount / 2 + 1);

//     // User deposits assets into the vault
//     vault.deposit_assets(user, amount, amount, None)?;
//     let total_lp_staked: u128 = vault.autocompounder_app.total_lp_position()?.into();
//     assert_that!(total_lp_staked).is_equal_to(150_002u128);

//     // Check the amount of vault token the user has
//     let user_vault_token_balance = vault.vault_token_balance(user_addr)?;
//     // Logic to calculate expected balance, adjusted to new framework's calculation method
//     // Placeholder for calculation logic
//     let expected_user_vault_token_balance = calculate_expected_balance(amount, total_lp_staked, attacker_initial_vt_balance);
//     assert_that!(user_vault_token_balance).is_equal_to(expected_user_vault_token_balance);

//     // Attacker redeems the initial deposit without unbonding
//     vault.redeem_vault_token(attacker_initial_vt_balance, attacker, None)?;
//     let attacker_pending_claims = vault.pending_claims(attacker_addr)?;
//     assert_that!(attacker_pending_claims).is_equal_to(attacker_initial_vt_balance);

//     Ok(())
// }

// pub fn vault_token_inflation_attack_full_dilute<Chain: CwEnv, Dex: DexInit>(
//     vault: GenericVault<Chain, Dex>,
//     user: &<Chain as TxHandler>::Sender,
//     user_addr: &Addr,
//     attacker: &<Chain as TxHandler>::Sender,
//     attacker_addr: &Addr,
// ) -> AResult {
//     let _ac_address = vault.autocompounder_app.addr_str()?;
//     let config: Config = vault.autocompounder_app.config()?;
//     let user_deposit = 100_000u128;
//     let attacker_deposit = 1u128;
//     let fully_dilute_donation = (10u128.pow(DECIMAL_OFFSET) * user_deposit - 1) * (attacker_deposit + 1) + 1;

//     // Assuming the new framework handles the setup and minting implicitly or it's done beforehand
//     // Deposit by attacker with minimal amount to attempt to dilute vault token value
//     vault.deposit_assets(attacker, attacker_deposit, 0, None)?;
//     let attacker_initial_vt_balance = vault.vault_token_balance(attacker_addr)?;

//     // Attacker makes a large donation to attempt full dilution
//     // Note: Assuming `donate_to_liquidity_pool` or equivalent method is available
//     donate_to_liquidity_pool(&vault, attacker, fully_dilute_donation)?;

//     // User attempts to deposit, expecting dilution to prevent meaningful vault token issuance
//     let deposit_attempt_result = vault.deposit_assets(user, user_deposit, 0, None);

//     // Check the result of the deposit attempt - expecting an error or zero minting based on your logic
//     // Adjust this assertion based on how your framework indicates a failed deposit due to full dilution
//     assert_that!(deposit_attempt_result).is_err();

//     Ok(())
// }

// // You may need a helper function or modify the existing method for donations if it doesn't exist
// // This is a placeholder for the logic to handle donations in the new framework
// fn donate_to_liquidity_pool<Chain: CwEnv, Dex: DexInit>(
//     vault: &GenericVault<Chain, Dex>,
//     donor: &<Chain as TxHandler>::Sender,
//     amount: u128,
// ) -> AResult {
//     // Placeholder for donation logic
//     // Adjust based on actual implementation
//     Ok(())
// }




fn require_unbonding_period(config: Config, state: bool) {
    if state && config.unbonding_period.is_none() {
        panic!("This test requires an unbonding period to be set in the config. Either remove this test or make sure its setup correctly.");
    }
    if !state && config.unbonding_period.is_some() {
        panic!("This test requires an unbonding period to be not set in the config. Either remove this test or make sure its setup correctly.");
    }
}
