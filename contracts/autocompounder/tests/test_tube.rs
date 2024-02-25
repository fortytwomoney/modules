#![cfg(feature = "test-tube")]
mod common;
use abstract_interface::AbstractInterfaceError;

use common::dexes::DexInit;
use common::dexes::IncentiveParams;
use common::integration::deposit_with_recipient;
use common::integration::test_deposit_assets;
use cw_asset::Asset;
use cw_asset::AssetInfo;
use cw_orch::osmosis_test_tube::osmosis_test_tube::Account;



use autocompounder::state::Config;
use cw_orch::prelude::*;

use autocompounder::msg::BondingData;
#[allow(unused_imports)]
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};

use common::dexes::OsmosisDex as OsmosisDexSetup;
use common::vault::GenericVault;
use common::AResult;
use cosmwasm_std::{Addr, Decimal};

use cw_utils::Duration;

use wyndex_bundle::*;

pub fn setup_osmosis_vault() -> Result<GenericVault<OsmosisTestTube, OsmosisDexSetup<OsmosisTestTube>>, AbstractInterfaceError> {
    let token_a = AssetInfo::native(EUR);
    let token_b = AssetInfo::native(USD);
    let reward_token = AssetInfo::native("uosmo");

    let ans_asset_references = vec![
        (EUR.to_string(), token_a.clone()),
        (USD.to_string(), token_b.clone()),
        ("uosmo".to_string(), reward_token.clone()),
    ];

    let initial_liquidity = vec![
        vec![
            Asset::new(token_a.clone(), 10_000u128),
            Asset::new(token_b.clone(), 10_000u128),
        ],
        vec![
            Asset::new(token_a.clone(), 10_000u128),
            Asset::new(reward_token.clone(), 10_000u128),
        ],
    ];

    let initial_accounts_balances = vec![(
        "account1",
        vec![
            Asset::new(token_a.clone(), 20_000u128),
            Asset::new(token_b.clone(), 10_000u128),
            Asset::new(reward_token.clone(), 10_000_000_000u128), // 10 osmo for gas?

        ],
    ),
    (
        "account2",
        vec![
            Asset::new(token_a.clone(), 10_000u128),
            Asset::new(token_b.clone(), 10_000u128),
            Asset::new(reward_token, 10_000_000_000u128), // 10 osmo for gas?
        ],
    ),
    (
        common::COMMISSION_RECEIVER,
        vec![],
    )];

    let incentive = IncentiveParams::from_coin(100u128, "uosmo", 1);


    let osmosis_setup = OsmosisDexSetup::setup_dex::<OsmosisTestTube>(
        ans_asset_references,
        initial_liquidity,
        initial_accounts_balances,
        incentive,
    ).unwrap();

    let instantiate_msg = autocompounder::msg::AutocompounderInstantiateMsg {
        code_id: None,
        commission_addr: osmosis_setup.accounts.get(2).unwrap().address().to_string(),
        deposit_fees: Decimal::percent(0),
        dex: osmosis_setup.name.clone(),
        performance_fees: Decimal::percent(3),
        pool_assets: osmosis_setup.dex_base.pools.first().unwrap().1.assets.clone(),
        withdrawal_fees: Decimal::percent(0),
        bonding_data: Some(BondingData {
            unbonding_period: Duration::Time(1),
            max_claims_per_address: None,
        }),
        max_swap_spread: Some(Decimal::percent(50)),
    };

    println!("instantiate_msg: {:?}", instantiate_msg);

    let vault = GenericVault::new(osmosis_setup.chain.clone(), osmosis_setup, &instantiate_msg)
        .map_err(|e| AbstractInterfaceError::Std(cosmwasm_std::StdError::GenericErr { msg: e.to_string() }))?;

    // TODO: Check autocompounder config
    let _config: Config = vault.autocompounder_app.config().unwrap();
    println!(" config: {:#?}", _config);

    Ok(vault)
}

#[test]
fn deposit_assets_native_osmosistesttube() -> AResult {
    let vault = setup_osmosis_vault().unwrap();

    let user1 = vault.dex.accounts[0].clone();
    let user2 = vault.dex.accounts[1].clone();
    let user1_addr = Addr::unchecked(user1.address());
    let user2_addr = Addr::unchecked(user2.address());

    test_deposit_assets(vault, &user1, &user1_addr, &user2, &user2_addr)
}

#[test]
fn deposit_with_recipient_osmosistesttube() -> AResult {
    let vault = setup_osmosis_vault().unwrap();

    let user1 = vault.dex.accounts[0].clone();
    let user2 = vault.dex.accounts[1].clone();
    let user1_addr = Addr::unchecked(user1.address());
    let user2_addr = Addr::unchecked(user2.address());

    deposit_with_recipient(vault, &user1, &user1_addr, &user2, &user2_addr)
}