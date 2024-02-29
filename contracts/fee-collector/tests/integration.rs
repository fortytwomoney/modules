use abstract_core::objects::gov_type::GovernanceDetails;
use std::str::FromStr;

use abstract_core::objects::AssetEntry;

use abstract_dex_adapter::contract::CONTRACT_VERSION as DEX_ADAPTER_VERSION;
use abstract_dex_adapter::msg::DexInstantiateMsg;
use abstract_dex_adapter::{interface::DexAdapter, DEX_ADAPTER_ID};

use abstract_interface::{
    Abstract, AbstractAccount, AbstractInterfaceError, AccountDetails, ManagerQueryFns,
};
use abstract_sdk::core::adapter::InstantiateMsg;
use abstract_testing::prelude::{EUR, USD};
use cw_orch::prelude::*;

use cosmwasm_std::{coin, Decimal};

use fee_collector::{contract::interface::FeeCollectorInterface, msg::FEE_COLLECTOR};
use fee_collector::{
    msg::{FeeCollectorExecuteMsgFns, FeeCollectorQueryMsgFns},
    state::Config,
};
use speculoos::{assert_that, prelude::ContainingIntoIterAssertions, vec::VecAssertions};
use wyndex_bundle::{WynDex, WYNDEX, WYND_TOKEN};

const COMMISSION_ADDR: &str = "commission_addr";
const TEST_NAMESPACE: &str = "4t2";
pub type AResult = anyhow::Result<()>;

// This is where you can do your integration tests for your module
pub struct App<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub fee_collector: FeeCollectorInterface<Chain>,
    pub dex: DexAdapter<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}

/// Instantiates the dex api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange(
    chain: MockBech32,
    deployment: &Abstract<MockBech32>,
    version: Option<String>,
) -> Result<DexAdapter<MockBech32>, AbstractInterfaceError> {
    let exchange = DexAdapter::new(DEX_ADAPTER_ID, chain);

    exchange.upload()?;
    exchange.instantiate(
        &InstantiateMsg {
            module: DexInstantiateMsg {
                swap_fee: Decimal::from_str("0.000")?,
                recipient_account: 0,
            },
            base: abstract_core::adapter::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version = version.unwrap_or_else(|| DEX_ADAPTER_VERSION.to_string());

    deployment
        .version_control
        .register_adapters(vec![(exchange.as_instance(), version)])?;
    Ok(exchange)
}

fn init_fee_collector(
    chain: MockBech32,
    deployment: &Abstract<MockBech32>,
    _version: Option<String>,
) -> Result<FeeCollectorInterface<MockBech32>, AbstractInterfaceError> {
    let fee_collector = FeeCollectorInterface::new(FEE_COLLECTOR, chain);

    fee_collector.upload()?;

    deployment
        .version_control
        .register_apps(vec![(
            fee_collector.as_instance(),
            env!("CARGO_PKG_VERSION").parse().unwrap(),
        )])
        .unwrap();
    Ok(fee_collector)
}

fn create_fee_collector(
    mock: MockBech32,
    allowed_assets: Vec<AssetEntry>,
) -> Result<App<MockBech32>, AbstractInterfaceError> {
    // Deploy abstract
    let abstract_ = Abstract::deploy_on(mock.clone(), mock.sender.to_string())?;

    // create first Account
    abstract_.account_factory.create_default_account(
        abstract_core::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;

    let _account = abstract_.account_factory.create_new_account(
        AccountDetails {
            account_id: None,
            namespace: Some(TEST_NAMESPACE.to_string()),
            description: None,
            link: None,
            base_asset: None,
            name: "Vault Account".to_string(),
            install_modules: vec![],
        },
        GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
        None,
    )?;

    // abstract_
    //     .version_control
    //     .claim_namespace(account.id()?, TEST_NAMESPACE.to_string())?;

    // Deploy mock dex
    let wyndex = WynDex::store_on(mock.clone()).unwrap();

    // Set up the dex and staking contracts
    let exchange_api = init_exchange(mock.clone(), &abstract_, None)?;
    let fee_collector = init_fee_collector(mock.clone(), &abstract_, None)?;

    // Create an Account that we will turn into a vault
    let account = abstract_.account_factory.create_default_account(
        abstract_core::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;

    // install dex
    account
        .manager
        .install_module::<Empty>(DEX_ADAPTER_ID, None, None)?;
    account.manager.install_module(
        FEE_COLLECTOR,
        Some(&fee_collector::msg::FeeCollectorInstantiateMsg {
            commission_addr: mock.addr_make(COMMISSION_ADDR).to_string(),
            max_swap_spread: Decimal::percent(25),
            fee_asset: EUR.to_string(),
            dex: WYNDEX.to_string(),
        }),
        None,
    )?;

    // get its address
    let fee_collector_addr = account
        .manager
        .module_addresses(vec![FEE_COLLECTOR.into()])?
        .modules[0]
        .1
        .clone();
    // set the address on the contract
    fee_collector.set_address(&fee_collector_addr);

    // give the autocompounder permissions to call on the dex and cw-staking contracts
    account.manager.update_adapter_authorized_addresses(
        DEX_ADAPTER_ID,
        vec![fee_collector_addr.to_string()],
        vec![],
    )?;

    let _fee_collector_config = fee_collector.config()?;

    // set allowed assets
    if !allowed_assets.is_empty() {
        fee_collector
            .call_as(&account.manager.address()?)
            .add_allowed_assets(allowed_assets)?;
    }

    Ok(App {
        account,
        fee_collector,
        abstract_core: abstract_,
        wyndex,
        dex: exchange_api,
    })
}

#[test]
fn test_update_config() -> AResult {
    let mock = MockBech32::new("mock");
    let commission_addr = mock.addr_make(COMMISSION_ADDR);
    let app = create_fee_collector(mock.clone(), vec![])?;

    let eur_asset = AssetEntry::new(EUR);
    let usd_asset = AssetEntry::new(USD);

    let wynd_asset = AssetEntry::new(WYND_TOKEN);
    let _unsupported_asset = AssetEntry::new("unsupported");

    app.fee_collector
        .call_as(&app.account.manager.address()?)
        .update_config(
            Some(mock.addr_make(COMMISSION_ADDR).to_string()),
            Some(WYNDEX.to_string()),
            Some(USD.to_string()),
            Some(Decimal::from_str("0.2")?),
        )?;

    let config: Config = app.fee_collector.config()?;
    assert_that!(config.fee_asset).is_equal_to(usd_asset.clone());
    assert_that!(config.dex).is_equal_to(WYNDEX.to_string());
    assert_that!(config.max_swap_spread).is_equal_to(Decimal::from_str("0.2")?);
    assert_that!(config.commission_addr).is_equal_to(commission_addr);

    // Adding fee asset is not allowed
    let _err = app
        .fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![eur_asset.clone(), usd_asset])
        .unwrap_err();

    // Adding no assets is not allowed
    let _err = app
        .fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![])
        .unwrap_err();

    // Adding non fee assets
    app.fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![eur_asset.clone()])?;
    let allowed_assets: Vec<AssetEntry> = app.fee_collector.allowed_assets()?;
    assert_that!(allowed_assets.len()).is_equal_to(1);
    assert_that!(allowed_assets).contains(eur_asset);

    // dex api doesnt support multi hop swaps and in the test case there is no wynd usd pool.
    app.fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![wynd_asset])
        .unwrap_err();

    // update allowed assets with assets that are not supported by the dex
    // let _err = app
    //     .fee_collector
    //     .call_as(&app.account.manager.address()?)
    //     .add_allowed_assets(vec![unsupported_asset])
    //     .unwrap_err();

    Ok(())
}

#[test]
fn test_collect_fees() -> AResult {
    let mock = MockBech32::new("mock");
    let non_admin = mock.addr_make("non-admin");
    let commission_addr = mock.addr_make(COMMISSION_ADDR);

    let _eur_asset = AssetEntry::new(EUR);
    let usd_asset = AssetEntry::new(USD);
    let wynd_token = AssetEntry::new(WYND_TOKEN);
    let app = create_fee_collector(mock.clone(), vec![usd_asset, wynd_token])?;

    mock.set_balance(
        &app.account.proxy.address()?,
        vec![
            coin(1_000u128, EUR),
            coin(1_000u128, USD),
            coin(1_000u128, WYND_TOKEN),
        ],
    )?;

    // not admin
    let _err = app.fee_collector.call_as(&non_admin).collect().unwrap_err();

    // call as admin
    // will swap 1K USD to EUR, 1K WYND to EUR. Both pools have 10K/10K ratio, so 10K swap leads to a spread 0f 129 which is 0.90%
    app.fee_collector
        .call_as(&app.account.manager.address()?)
        .collect()?;

    let fee_balances = mock.query_all_balances(&app.account.proxy.address()?)?;
    assert_that!(fee_balances).is_empty();

    // swap of wynd->eur and usd->eur of 1K each lead to 2 * 909 = 1818 eur. This + the 1K eur that was already in the account
    let expected_usd_balance = coin(2818u128, EUR);
    let commission_balances = mock.query_all_balances(&commission_addr)?;
    let usd_balance = commission_balances.first().unwrap();
    assert_that!(commission_balances).has_length(1);
    assert_that!(usd_balance).is_equal_to(&expected_usd_balance);

    Ok(())
}

#[test]
#[ignore = "Multipool hops need a router contract... Not supported yet"]
fn test_add_allowed_assets() -> AResult {
    let mock = MockBech32::new("mock");

    let eur_asset = AssetEntry::new(EUR);
    let usd_asset = AssetEntry::new(USD);
    let wynd_token = AssetEntry::new(WYND_TOKEN);
    let app = create_fee_collector(mock, vec![usd_asset.clone(), wynd_token.clone()])?;

    // not admin
    let _err = app
        .fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![eur_asset.clone()])
        .unwrap_err();

    // call as admin
    app.fee_collector
        .call_as(&app.account.manager.address()?)
        .add_allowed_assets(vec![eur_asset.clone()])?;

    let allowed_assets: Vec<AssetEntry> = app.fee_collector.allowed_assets()?;
    assert_that!(allowed_assets.len()).is_equal_to(3);
    assert_that!(allowed_assets).contains(eur_asset);
    assert_that!(allowed_assets).contains(usd_asset);
    assert_that!(allowed_assets).contains(wynd_token);

    Ok(())
}
