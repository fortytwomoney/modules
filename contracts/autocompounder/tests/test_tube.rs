#![cfg(feature = "test-tube")]

mod common;
const DECIMAL_OFFSET: u32 = 17;

use std::str::FromStr;

use abstract_client::{AbstractClient, Namespace};
use abstract_core::objects::{pool_id::PoolAddressBase, PoolMetadata};
use abstract_core::objects::{AnsAsset, AssetEntry};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::{interface::DexAdapter, msg::DexInstantiateMsg};
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::BondingData;
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};
use autocompounder::state::Config;
use common::{AResult, TEST_NAMESPACE};
use cosmwasm_std::{coin, Addr, Decimal, Uint128};
use cw_asset::AssetInfo;
use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::prelude::*;
use cw_orch::{
    environment::{BankQuerier, TxHandler},
    osmosis_test_tube::{osmosis_test_tube::Account, OsmosisTestTube},
};
use cw_utils::Duration;

mod vault {
    use super::*;
    use abstract_client::{Account, Application, Publisher};
    use cosmwasm_std::Coin;
    use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;
    pub struct VaultOsmosis {
        pub chain: OsmosisTestTube,
        // Account with sub-account autocompounder
        pub account: Account<OsmosisTestTube>,
        // Autocompounder app on account
        pub autocompounder_app: Application<OsmosisTestTube, AutocompounderApp<OsmosisTestTube>>,
        pub fortytwo_publisher: Publisher<OsmosisTestTube>,
        pub commission_addr: Addr,
        pub dex: OsmosisDex,
        pub abstract_client: AbstractClient<OsmosisTestTube>,
        // OsmosisTestTube don't give us access to set balances
        pub wallet: std::rc::Rc<SigningAccount>,
    }

    impl VaultOsmosis {
        pub fn add_balance(
            &self,
            address: &Addr,
            amount: Vec<Coin>,
        ) -> Result<cw_orch::mock::cw_multi_test::AppResponse, CwOrchError> {
            self.chain
                .call_as(&self.wallet)
                .bank_send(address.to_string(), amount)
        }
    }

    #[derive(Clone)]
    pub struct OsmosisDex {
        pub eur_token: AssetInfo,
        pub usd_token: AssetInfo,
        pub eur_usd_pool_id: u64,
    }
}
use vault::{OsmosisDex, VaultOsmosis};

pub const EUR: &str = "eur";
pub const USD: &str = "usd";
pub const DEX: &str = "osmosis";

#[allow(unused)]
mod debug {
    // Put it in any part of your code to enable logs
    fn enable_debug_logs() {
        std::env::set_var("RUST_LOG", "debug");
        let _ = env_logger::builder().is_test(true).try_init();
    }

    fn disable_debug_logs() {
        std::env::remove_var("RUST_LOG")
    }
}

fn setup_vault() -> anyhow::Result<VaultOsmosis> {
    let mut chain = OsmosisTestTube::new(vec![coin(1_000_000_000_000, "uosmo")]);
    // No access to set balance on test-tube
    let wallet = chain.init_account(vec![
        coin(1_000_000_000_000, "uosmo"),
        coin(1_000_000_000_000, EUR),
        coin(1_000_000_000_000, USD),
    ])?;

    let pool_id = chain
        .call_as(&wallet)
        .create_pool(vec![coin(10_000, EUR), coin(10_000, USD)])?;
    let eur_token = AssetInfo::native(EUR);
    let usd_token = AssetInfo::native(USD);

    let abstract_client = AbstractClient::builder(chain.clone())
        .assets(vec![
            (EUR.to_owned(), eur_token.clone().into()),
            (USD.to_owned(), usd_token.clone().into()),
            (
                format!("{DEX}/{EUR},{USD}"),
                AssetInfo::native(format!("gamm/pool/{pool_id}")).into(),
            ),
        ])
        .dex(DEX)
        .pools(vec![(
            PoolAddressBase::id(pool_id),
            PoolMetadata::stable(DEX, vec![EUR, USD]),
        )])
        .build()?;

    let abstract_publisher = abstract_client
        .publisher_builder(Namespace::new("abstract")?)
        .build()?;

    let _exchange: DexAdapter<_> = abstract_publisher.publish_adapter(DexInstantiateMsg {
        swap_fee: Decimal::from_str("0.003")?,
        recipient_account: 0,
    })?;
    let _staking: CwStakingAdapter<_> = abstract_publisher.publish_adapter(Empty {})?;

    let fortytwo_publisher = abstract_client
        .publisher_builder(Namespace::new(TEST_NAMESPACE)?)
        .build()?;
    fortytwo_publisher.publish_app::<AutocompounderApp<_>>()?;

    let commission_account = chain.init_account(vec![coin(1_000_000_000_000, "uosmo")])?;
    let commission_addr = Addr::unchecked(commission_account.address());

    let account = abstract_client
        .account_builder()
        .install_on_sub_account(true)
        .build()?;

    let autocompounder_app = account.install_app_with_dependencies::<AutocompounderApp<_>>(
        &autocompounder::msg::AutocompounderInstantiateMsg {
            code_id: None,
            commission_addr: commission_addr.to_string(),
            deposit_fees: Decimal::percent(0),
            dex: DEX.to_string(),
            performance_fees: Decimal::percent(3),
            pool_assets: vec![AssetEntry::new(EUR), AssetEntry::new(USD)],
            withdrawal_fees: Decimal::percent(0),
            bonding_data: Some(BondingData {
                unbonding_period: Duration::Time(1),
                max_claims_per_address: None,
            }),
            max_swap_spread: Some(Decimal::percent(50)),
        },
        cosmwasm_std::Empty {},
        &[],
    )?;

    _exchange.execute(
        &abstract_dex_adapter::msg::ExecuteMsg::Base(abstract_core::adapter::BaseExecuteMsg {
            proxy_address: Some(autocompounder_app.account().proxy()?.to_string()),
            msg: abstract_core::adapter::AdapterBaseMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_app.addr_str()?],
                to_remove: vec![],
            },
        }),
        None,
    )?;
    _staking.execute(
        &abstract_cw_staking::msg::ExecuteMsg::Base(abstract_core::adapter::BaseExecuteMsg {
            proxy_address: Some(autocompounder_app.account().proxy()?.to_string()),
            msg: abstract_core::adapter::AdapterBaseMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_app.addr_str()?],
                to_remove: vec![],
            },
        }),
        None,
    )?;

    let osmosis_dex = OsmosisDex {
        eur_token,
        usd_token,
        eur_usd_pool_id: pool_id,
    };

    Ok(VaultOsmosis {
        chain,
        account,
        autocompounder_app,
        fortytwo_publisher,
        commission_addr,
        dex: osmosis_dex,
        abstract_client,
        wallet,
    })
}

// TODO: finish test
#[test]
fn deposit_asset() -> AResult {
    let vault = setup_vault()?;
    let mut chain = vault.chain.clone();

    let OsmosisDex {
        eur_token,
        usd_token,
        ..
    } = vault.dex.clone();

    let ac_addres = vault.autocompounder_app.address()?;
    let owner = chain.sender();
    let usd_asset = AssetEntry::new(USD);
    let eur_asset = AssetEntry::new(EUR);

    let asset_entries = vec![eur_asset.clone(), usd_asset.clone()];
    let asset_infos = vec![eur_token.clone(), usd_token.clone()];

    let config: Config = vault.autocompounder_app.config()?;

    // check the config
    assert_eq!(config.pool_data.assets, asset_entries);
    assert_eq!(config.pool_assets, asset_infos);

    // deposit 10_000 usd and eur (native-native)
    let amount = 10_000u128;
    vault.add_balance(&owner, vec![coin(amount, EUR), coin(amount, USD)])?;
    let user1 = chain.init_account(vec![coin(amount, EUR)])?;

    vault.add_balance(&ac_addres, vec![coin(amount, EUR), coin(amount, USD)])?;

    vault.autocompounder_app.deposit(
        vec![
            AnsAsset::new(eur_asset, amount),
            AnsAsset::new(usd_asset.clone(), amount),
        ],
        None,
        None,
        &[coin(amount, EUR), coin(amount, USD)],
    )?;

    let position: Uint128 = vault.autocompounder_app.total_lp_position()?;
    assert_eq!(position, Uint128::from(100_000_000_000_000_000_000u128));

    let AssetInfo::Native(denom) = config.vault_token else {
        panic!("Expected factory token")
    };
    let balance_owner: Uint128 = chain.query_balance(owner.as_str(), denom.as_str())?;
    assert_eq!(
        balance_owner.u128(),
        10_000u128 * 10u128.pow(DECIMAL_OFFSET)
    );

    // single asset deposit from different address
    // raw_token
    //     .call_as(&user1)
    //     .increase_allowance(1000u128.into(), _ac_addres.to_string(), None)?;
    // vault.auto_compounder.call_as(&user1).deposit(
    //     vec![AnsAsset::new(usd_asset, 1000u128)],
    //     None,
    //     None,
    //     &[],
    // )?;

    // // check that the vault token is minted
    // let vault_token_balance = vault.vault_token.balance(owner.to_string())?;
    // assert_that!(vault_token_balance.balance.u128())
    //     .is_equal_to(10000u128 * 10u128.pow(DECIMAL_OFFSET));
    // let new_position = vault.auto_compounder.total_lp_position()?;
    // // check if the user1 balance is correct
    // let vault_token_balance_user1 = vault.vault_token.balance(user1.to_string())?;
    // assert_that!(vault_token_balance_user1.balance.u128())
    //     .is_equal_to(487u128 * 10u128.pow(DECIMAL_OFFSET));
    // assert_that!(new_position).is_greater_than(position);

    // let redeem_amount = Uint128::from(4000u128 * 10u128.pow(DECIMAL_OFFSET));
    // vault
    //     .vault_token
    //     .call_as(&owner)
    //     .increase_allowance(redeem_amount, _ac_addres, None)?;
    // vault.auto_compounder.redeem(redeem_amount, None)?;

    // // check that the vault token decreased
    // let vault_token_balance = vault.vault_token.balance(owner.to_string())?;
    // assert_that!(vault_token_balance.balance.u128())
    //     .is_equal_to(6000u128 * 10u128.pow(DECIMAL_OFFSET));

    Ok(())
}
