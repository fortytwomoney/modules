mod common;

use std::str::FromStr;

use abstract_core::objects::pool_id::PoolAddressBase;
use abstract_core::objects::UncheckedContractEntry;
use abstract_interface::AbstractInterfaceError;

use autocompounder::error::AutocompounderError;
use autocompounder::state::DECIMAL_OFFSET;
use common::dexes::DexInit;
use common::dexes::WyndDex as SetupWyndDex;
use cw_asset::Asset;
use cw_asset::AssetInfo;
use cw_asset::AssetInfoBase;
use cw_plus_interface::cw20_base::Cw20Base;

use abstract_core::objects::{LpToken, PoolMetadata};
use abstract_interface::Abstract;

use autocompounder::state::Config;
use cw_orch::prelude::*;

use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns, BondingData};

use common::vault::{AssetWithInfo, GenericDex, GenericVault};
use common::AResult;
use common::VAULT_TOKEN;
use cosmwasm_std::{Addr, Decimal, Uint128};

use cw_utils::Duration;

use cw_orch::deploy::Deploy;
use speculoos::assert_that;
use wyndex_bundle::*;

const WYNDEX: &str = "wyndex";
const COMMISSION_RECEIVER: &str = "commission_receiver";
const ATTACKER: &str = "attacker";
/// Convert vault tokens to lp assets
pub fn convert_to_assets(
    shares: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    shares.multiply_ratio(
        total_assets + Uint128::from(1u128),
        total_supply + Uint128::from(10u128).pow(decimal_offset),
    )
}
pub fn cw20_lp_token(liquidity_token: AssetInfoBase<Addr>) -> Result<Addr, AutocompounderError> {
    match liquidity_token {
        AssetInfoBase::Cw20(contract_addr) => Ok(contract_addr),
        _ => Err(AutocompounderError::SenderIsNotLpToken {}),
    }
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
pub fn convert_to_shares(
    assets: Uint128,
    total_assets: Uint128,
    total_supply: Uint128,
    decimal_offset: u32,
) -> Uint128 {
    assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(decimal_offset),
        total_assets + Uint128::from(1u128),
    )
}

fn setup_mock_cw20_vault() -> Result<GenericVault<Mock>, AbstractInterfaceError> {
    let owner = Addr::unchecked(common::OWNER);
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);
    let user1 = Addr::unchecked(common::USER1);
    let mock = Mock::new(&owner);
    let abstract_ = Abstract::deploy_on(mock.clone(), mock.sender().to_string())?;
    let wyndex = WynDex::store_on(mock.clone()).unwrap();

    let WynDex {
        raw_token,
        raw_2_token,
        eur_token,
        usd_token,
        wynd_token,
        eur_usd_lp,
        raw_eur_lp,
        wynd_eur_lp,
        raw_raw_2_lp,
        raw_raw_2_staking,
        ..
    } = wyndex;

    let assets: Vec<AssetWithInfo> = vec![
        (
            RAW_TOKEN.to_string(),
            AssetInfoBase::cw20(raw_token.address()?),
        ),
        (
            RAW_2_TOKEN.to_string(),
            AssetInfoBase::cw20(raw_2_token.address()?),
        ),
        (EUR.to_string(), AssetInfoBase::native(EUR)),
        (USD.to_string(), AssetInfoBase::native(USD)),
        (
            WYND_TOKEN.to_string(),
            AssetInfoBase::cw20(Addr::unchecked(WYND_TOKEN)),
        ),
        (
            LpToken::new(WYNDEX, vec![EUR, USD]).to_string(),
            AssetInfoBase::cw20(eur_usd_lp.address()?),
        ),
        (
            LpToken::new(WYNDEX, vec![RAW_TOKEN, EUR]).to_string(),
            AssetInfoBase::cw20(raw_eur_lp.address()?),
        ),
        (
            LpToken::new(WYNDEX, vec![EUR, WYND_TOKEN]).to_string(),
            AssetInfoBase::cw20(wynd_eur_lp.address()?),
        ),
        (
            LpToken::new(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]).to_string(),
            AssetInfoBase::cw20(raw_raw_2_lp.address()?),
        ),
    ]
    .iter()
    .map(|(ans_name, asset_info)| AssetWithInfo::new(ans_name, asset_info.clone()))
    .collect();

    let vault_pool = (
        PoolAddressBase::contract(Addr::unchecked("raw_raw_2_pair")),
        PoolMetadata::stable(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]),
    );

    let swap_pools = vec![
        (
            PoolAddressBase::contract(Addr::unchecked("eur_usd_pair")),
            PoolMetadata::stable(WYNDEX, vec![EUR, USD]),
        ),
        (
            PoolAddressBase::contract(Addr::unchecked("raw_eur_pair")),
            PoolMetadata::stable(WYNDEX, vec![RAW_TOKEN, EUR]),
        ),
        (
            PoolAddressBase::contract(Addr::unchecked("wynd_eur_pair")),
            PoolMetadata::stable(WYNDEX, vec![WYND_TOKEN, EUR]),
        ),
    ];

    let raw_raw_2_lp_asset = LpToken::new(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]);
    let contracts = vec![(
        UncheckedContractEntry::new(WYNDEX.to_string(), format!("staking/{raw_raw_2_lp_asset}")),
        raw_raw_2_staking.to_string(),
    )];

    let mut wyndex_setup = SetupWyndDex {
        chain: mock.clone(),
        assets: assets.iter().map(|f| f.asset_info.clone()).collect(),
        cw20_minter: wyndex_owner,
        name: "wyndex".to_string(),
    };

    // in the case of wyndex all the pools are already setup in the wyndex bundle.
    wyndex_setup.setup_pools(vec![]).unwrap();

    // TODO: set balances for test users and env
    wyndex_setup.set_balances(&vec![]).unwrap();

    let pools = [vec![vault_pool.clone()], swap_pools].concat();
    let vault_token = Cw20Base::new(VAULT_TOKEN, mock.clone());
    let cw20_id = vault_token.upload().unwrap().uploaded_code_id().unwrap();

    let instantiate_msg = autocompounder::msg::AutocompounderInstantiateMsg {
        code_id: Some(cw20_id),
        commission_addr: COMMISSION_RECEIVER.to_string(),
        deposit_fees: Decimal::percent(0),
        dex: WYNDEX.to_string(),
        performance_fees: Decimal::percent(3),
        pool_assets: vault_pool.1.assets.clone(),
        withdrawal_fees: Decimal::percent(0),
        bonding_data: Some(BondingData {
            unbonding_period: Duration::Time(1),
            max_claims_per_address: None,
        }),
        max_swap_spread: Some(Decimal::percent(50)),
    };

    let dex = GenericDex {
        assets: assets.clone(),
        pools,
        contracts,
        dex_name: WYNDEX.to_string(),
    };

    let vault = GenericVault::new(mock, assets, dex, &instantiate_msg).unwrap();

    // TODO: Check autocompounder config
    let config: Config = vault.autocompounder_app.config().unwrap();

    Ok(vault)
}

fn setup_mock_native_vault() -> Result<GenericVault<Mock>, AbstractInterfaceError> {
    let owner = Addr::unchecked(common::OWNER);
    let wyndex_owner = Addr::unchecked(WYNDEX_OWNER);
    let user1 = Addr::unchecked(common::USER1);
    let mock = Mock::new(&owner);
    let abstract_ = Abstract::deploy_on(mock.clone(), mock.sender().to_string())?;
    let wyndex = WynDex::store_on(mock.clone()).unwrap();

    let WynDex {
        // eur_token,
        // usd_token,
        // wynd_token,
        wynd_eur_lp,
        wynd_eur_pair,
        eur_usd_pair,
        // eur_usd_lp,
        eur_usd_staking,
        ..
    } = wyndex;

    let pools = vec![
        (
            PoolAddressBase::contract(eur_usd_pair),
            PoolMetadata::stable(WYNDEX, vec![EUR, USD]),
        ),
        (
            PoolAddressBase::contract(wynd_eur_pair),
            PoolMetadata::stable(WYNDEX, vec![WYND_TOKEN, EUR]),
        ),
    ];

    let assets: Vec<AssetWithInfo> = vec![
        (EUR.to_string(), AssetInfoBase::native(EUR)),
        (USD.to_string(), AssetInfoBase::native(USD)),
        (
            WYND_TOKEN.to_string(),
            AssetInfoBase::cw20(Addr::unchecked(WYND_TOKEN)),
        ),
        (
            LpToken::new(WYNDEX, vec![EUR, WYND_TOKEN]).to_string(),
            AssetInfoBase::cw20(wynd_eur_lp.address()?),
        ),
        (
            LpToken::new(WYNDEX, vec![EUR, USD]).to_string(),
            AssetInfoBase::cw20(wynd_eur_lp.address()?),
        ),
    ]
    .into_iter()
    .map(|f| AssetWithInfo::new(f.0, f.1))
    .collect();

    let eur_usd_lp_asset = LpToken::new(WYNDEX, vec![EUR, USD]);
    let contracts = vec![(
        UncheckedContractEntry::new(WYNDEX.to_string(), format!("staking/{eur_usd_lp_asset}")),
        eur_usd_staking.to_string(),
    )];

    let mut wyndex_setup = SetupWyndDex {
        chain: mock.clone(),
        assets: assets.iter().map(|f| f.asset_info.clone()).collect(),
        cw20_minter: wyndex_owner,
        name: "wyndex".to_string(),
    };

    wyndex_setup.setup_pools(vec![]).unwrap();
    wyndex_setup.set_balances(&[(&owner, &vec![
        Asset::new(AssetInfo::native(USD), 10_000u128),
        Asset::new(AssetInfo::native(EUR), 10_000u128)
    ])]).unwrap();

    let vault_token = Cw20Base::new(VAULT_TOKEN, mock.clone());
    let cw20_id = vault_token.upload().unwrap().uploaded_code_id().unwrap();

    let instantiate_msg = autocompounder::msg::AutocompounderInstantiateMsg {
        code_id: Some(cw20_id),
        commission_addr: COMMISSION_RECEIVER.to_string(),
        deposit_fees: Decimal::percent(0),
        dex: WYNDEX.to_string(),
        performance_fees: Decimal::percent(3),
        pool_assets: pools.first().unwrap().1.assets.clone(),
        withdrawal_fees: Decimal::percent(0),
        bonding_data: Some(BondingData {
            unbonding_period: Duration::Time(1),
            max_claims_per_address: None,
        }),
        max_swap_spread: Some(Decimal::percent(50)),
    };

    let dex = GenericDex {
        assets: assets.clone(),
        pools,
        contracts,
        dex_name: WYNDEX.to_string(),
    };

    let vault = GenericVault::new(mock, assets, dex, &instantiate_msg).unwrap();

    // TODO: Check autocompounder config
    let config: Config = vault.autocompounder_app.config().unwrap();

    Ok(vault)
}

// #[test]
// fn deposit_assets_native_osmosistesttube() -> AResult {
//     let vault = setup_osmosis_vault()?;

//     let owner = Addr::unchecked(common::OWNER);
//     let user1 = Addr::unchecked(common::USER1);

//     test_deposit_assets(vault, &owner, &owner, &user1, &user1)
// }

#[test]
fn deposit_assets_cw20_mock() -> AResult {
    let vault = setup_mock_cw20_vault()?;
    let owner = Addr::unchecked(common::OWNER);
    let user1 = Addr::unchecked(common::USER1);
    test_deposit_assets(vault, &owner, &owner, &user1, &user1)
}

#[test]
fn deposit_assets_native_mock() -> AResult {
    let vault = setup_mock_native_vault()?;
    let owner = Addr::unchecked(common::OWNER);
    let user1 = Addr::unchecked(common::USER1);
    test_deposit_assets(vault, &owner,&owner, &user1,&user1)
}

fn test_deposit_assets<Chain: CwEnv>(
    vault: GenericVault<Chain>,
    owner: &<Chain as TxHandler>::Sender,
    owner_addr: &Addr,
    user: &<Chain as TxHandler>::Sender,
    user_addr: &Addr,
) -> AResult {
    let _ac_addres = vault.autocompounder_app.addr_str()?;
    let config: Config = vault.autocompounder_app.config()?;

    // deposit 10_000 of both assets
    let amount = 10_000u128;
    vault.deposit_assets(owner, amount, amount)?;

    let position = vault.autocompounder_app.total_lp_position()?;
    assert_that!(position).is_equal_to(Uint128::from(10_000u128));

    let balance_owner = vault.vault_token_balance(owner_addr.to_string())?;
    assert_that!(balance_owner).is_equal_to(10_000u128 * 10u128.pow(DECIMAL_OFFSET));

    // single cw20asset deposit from different address
    // single asset deposit from different address
    // raw_token
    //     .call_as(&user1)
    //     .increase_allowance(1000u128.into(), _ac_addres.to_string(), None)?;
    // vault.autocompounder_app.call_as(&user1).deposit(
    //     vec![AnsAsset::new(raw_asset, 1000u128)],
    //     None,
    //     None,
    //     &[],
    // )?;


    // // check that the vault token is minted
    // let vault_token_balance = vault.vault_token.balance(owner.to_string())?;
    // assert_that!(vault_token_balance.balance.u128())
    //     .is_equal_to(10000u128 * 10u128.pow(DECIMAL_OFFSET));
    // let new_position = vault.autocompounder_app.total_lp_position()?;
    // // check if the user1 balance is correct
    // let vault_token_balance_user1 = vault.vault_token.balance(user1.to_string())?;
    // assert_that!(vault_token_balance_user1.balance.u128())
    //     .is_equal_to(487u128 * 10u128.pow(DECIMAL_OFFSET));
    // // assert_that!(new_position).is_greater_than(position);

    // let redeem_amount = Uint128::from(4000u128 * 10u128.pow(DECIMAL_OFFSET));
    // vault
    //     .vault_token
    //     .call_as(&owner)
    //     .increase_allowance(redeem_amount, _ac_addres, None)?;
    // vault.autocompounder_app.redeem(redeem_amount, None, &[])?;

    // // check that the vault token decreased
    // let vault_token_balance = vault.vault_token.balance(owner.to_string())?;
    // assert_that!(vault_token_balance.balance.u128())
    //     .is_equal_to(6000u128 * 10u128.pow(DECIMAL_OFFSET));

    Ok(())
}
