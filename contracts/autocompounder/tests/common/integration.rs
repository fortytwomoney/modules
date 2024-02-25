use super::dexes::DexInit;
use autocompounder::state::DECIMAL_OFFSET;

use autocompounder::state::Config;
use cw_orch::prelude::*;

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

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    let amount = 10_000u128;
    vault.deposit_assets(user1, amount, amount, None)?;

    let u1_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u1_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_balance).is_equal_to(u1_lp_token_amount.u128() * 10u128.pow(DECIMAL_OFFSET));

    // deposit `amount` of only the first asset and check whether the vault tokens of u1 have increased
    vault.deposit_assets(user1, amount, 0, None)?;

    let u1_new_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u1_new_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_new_lp_token_amount - u1_lp_token_amount).is_greater_than(Uint128::zero());
    assert_that!(u1_new_balance - u1_balance).is_greater_than(0u128);
    // TODO: check whether the balance is exactly what it should be here

    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user2, amount, amount, None)?;
    let u2_lp_token_amount: Uint128 =
        vault.autocompounder_app.total_lp_position()? - u1_new_lp_token_amount;
    let u2_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u2_lp_token_amount).is_equal_to(u1_lp_token_amount);
    assert_that!(u2_balance).is_equal_to(u1_balance);

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

    // deposit `amount` of both assets and check whether the exact right amount of vault tokens are minted
    let amount = 10_000u128;
    vault.deposit_assets(user1, amount, amount, Some(user2_addr.clone()))?;

    let u1_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u2_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u2_balance).is_equal_to(u1_lp_token_amount.u128() * 10u128.pow(DECIMAL_OFFSET));

    // deposit `amount` of only the first asset and check whether the vault tokens of u2 have increased
    vault.deposit_assets(user1, amount, 0, Some(user2_addr.clone()))?;

    let u1_new_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u2_new_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u1_new_lp_token_amount - u1_lp_token_amount).is_greater_than(Uint128::zero());
    assert_that!(u2_new_balance - u2_balance).is_greater_than(0u128);

    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user2, amount, amount, Some(user1_addr.clone()))?;
    let u2_lp_token_amount: Uint128 =
        vault.autocompounder_app.total_lp_position()? - u1_new_lp_token_amount;
    let u1_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u2_lp_token_amount).is_equal_to(u1_lp_token_amount);
    assert_that!(u1_balance).is_equal_to(u2_balance);

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
    // todo: check wether the incentives are set to zero.

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
    
    let (u2_a_initial_balance, u2_b_initial_balance) = vault.asset_balances(user2_addr)?;
    
    // Same test as above but with recipient
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

fn require_unbonding_period(config: Config, state: bool) {
    if state && config.unbonding_period.is_none() {
        panic!("This test requires an unbonding period to be set in the config. Either remove this test or make sure its setup correctly.");
    }
    if !state && config.unbonding_period.is_some() {
        panic!("This test requires an unbonding period to be not set in the config. Either remove this test or make sure its setup correctly.");
    }
}
