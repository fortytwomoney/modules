

use autocompounder::state::DECIMAL_OFFSET;
use super::dexes::DexInit;


use autocompounder::state::Config;
use cw_orch::prelude::*;


use super::vault::GenericVault;
use super::AResult;
use cosmwasm_std::{Addr, Uint128};
#[allow(unused_imports)]
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};

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
    vault.deposit_assets(user1, amount, amount)?;

    let u1_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u1_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_balance).is_equal_to(u1_lp_token_amount.u128() * 10u128.pow(DECIMAL_OFFSET));

    // deposit `amount` of only the first asset and check whether the vault tokens of u1 have increased
    vault.deposit_assets(user1, amount, 0)?;

    let u1_new_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()?;
    let u1_new_balance = vault.vault_token_balance(user1_addr.to_string())?;
    assert_that!(u1_new_lp_token_amount - u1_lp_token_amount).is_greater_than(Uint128::zero());
    assert_that!(u1_new_balance - u1_balance).is_greater_than(0u128);
    // TODO: check whether the balance is exactly what it should be here


    // deposit `amount` of both assets by user2 and check whether the exact right amount of vault tokens are minted
    vault.deposit_assets(user2, amount, amount)?;
    let u2_lp_token_amount: Uint128 = vault.autocompounder_app.total_lp_position()? - u1_new_lp_token_amount;
    let u2_balance = vault.vault_token_balance(user2_addr.to_string())?;
    assert_that!(u2_lp_token_amount).is_equal_to(u1_lp_token_amount);
    assert_that!(u2_balance).is_equal_to(u1_balance);


    Ok(())
}