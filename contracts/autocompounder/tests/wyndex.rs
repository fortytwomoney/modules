// mod common;
// use abstract_core::objects::pool_id::PoolAddressBase;
// use abstract_core::objects::UncheckedContractEntry;
// use abstract_interface::AbstractInterfaceError;

// use autocompounder::error::AutocompounderError;
// use common::dexes::get_id_from_osmo_pool;
// use common::dexes::DexBase;
// use common::dexes::DexInit;
// use common::integration::test_deposit_assets;
// use cw_asset::Asset;
// use cw_asset::AssetInfo;
// use cw_asset::AssetInfoBase;
// use cw_plus_interface::cw20_base::Cw20Base;

// use abstract_core::objects::{LpToken, PoolMetadata};
// use abstract_interface::Abstract;

// use autocompounder::state::Config;
// use cw_orch::prelude::*;

// use autocompounder::msg::BondingData;
// use autocompounder::msg::AutocompounderQueryMsgFns;

// use common::dexes::WyndDex as SetupWyndDex;

// use common::vault::{AssetWithInfo, GenericVault};
// use common::AResult;
// use common::VAULT_TOKEN;
// use cosmwasm_std::{Addr, Decimal, Uint128};

// use cw_utils::Duration;

// use wyndex_bundle::*;

// const WYNDEX: &str = "wyndex";
// const ATTACKER: &str = "attacker";

// pub fn cw20_lp_token(liquidity_token: AssetInfoBase<Addr>) -> Result<Addr, AutocompounderError> {
//     match liquidity_token {
//         AssetInfoBase::Cw20(contract_addr) => Ok(contract_addr),
//         _ => Err(AutocompounderError::SenderIsNotLpToken {}),
//     }
// }

// fn setup_mock_cw20_vault() -> Result<GenericVault<MockBech32, SetupWyndDex<MockBech32>>, AbstractInterfaceError> {
//     let mock = MockBech32::new(common::OWNER);
//     let owner = mock.sender();
//     let wyndex_owner = mock.addr_make(WYNDEX_OWNER);
//     let _user1 = mock.addr_make(common::USER1);
//     let _abstract_ = Abstract::deploy_on(mock.clone(), mock.sender().to_string())?;
//     let wyndex = WynDex::store_on(mock.clone()).unwrap();

//     let WynDex {
//         raw_token,
//         raw_2_token,
        
        
        
//         eur_usd_lp,
//         raw_eur_lp,
//         wynd_eur_lp,
//         raw_raw_2_lp,
//         raw_raw_2_staking,
//         ..
//     } = wyndex;

//     let assets: Vec<AssetWithInfo> = vec![
//         (
//             RAW_TOKEN.to_string(),
//             AssetInfoBase::cw20(raw_token.address()?),
//         ),
//         (
//             RAW_2_TOKEN.to_string(),
//             AssetInfoBase::cw20(raw_2_token.address()?),
//         ),
//         (EUR.to_string(), AssetInfoBase::native(EUR)),
//         (USD.to_string(), AssetInfoBase::native(USD)),
//         (
//             WYND_TOKEN.to_string(),
//             AssetInfoBase::cw20(Addr::unchecked(WYND_TOKEN)),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![EUR, USD]).to_string(),
//             AssetInfoBase::cw20(eur_usd_lp.address()?),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![RAW_TOKEN, EUR]).to_string(),
//             AssetInfoBase::cw20(raw_eur_lp.address()?),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![EUR, WYND_TOKEN]).to_string(),
//             AssetInfoBase::cw20(wynd_eur_lp.address()?),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]).to_string(),
//             AssetInfoBase::cw20(raw_raw_2_lp.address()?),
//         ),
//     ]
//     .iter()
//     .map(|(ans_name, asset_info)| AssetWithInfo::new(ans_name, asset_info.clone()))
//     .collect();

//     let vault_pool = (
//         PoolAddressBase::contract(Addr::unchecked("raw_raw_2_pair")),
//         PoolMetadata::stable(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]),
//     );

//     let swap_pools = vec![
//         (
//             PoolAddressBase::contract(Addr::unchecked("eur_usd_pair")),
//             PoolMetadata::stable(WYNDEX, vec![EUR, USD]),
//         ),
//         (
//             PoolAddressBase::contract(Addr::unchecked("raw_eur_pair")),
//             PoolMetadata::stable(WYNDEX, vec![RAW_TOKEN, EUR]),
//         ),
//         (
//             PoolAddressBase::contract(Addr::unchecked("wynd_eur_pair")),
//             PoolMetadata::stable(WYNDEX, vec![WYND_TOKEN, EUR]),
//         ),
//     ];

//     let raw_raw_2_lp_asset = LpToken::new(WYNDEX, vec![RAW_TOKEN, RAW_2_TOKEN]);
//     let contracts = vec![(
//         UncheckedContractEntry::new(WYNDEX.to_string(), format!("staking/{raw_raw_2_lp_asset}")),
//         raw_raw_2_staking.to_string(),
//     )];
//     let pools = [vec![vault_pool.clone()], swap_pools].concat();

//     let mut wyndex_setup = SetupWyndDex {
//         chain: mock.clone(),
//         dex_base: DexBase {
//             pools,
//             contracts,
//             assets,
//             reward_tokens: vec![],
//         },
//         cw20_minter: wyndex_owner,
//         name: "wyndex".to_string(),
//     };

//     // in the case of wyndex all the pools are already setup in the wyndex bundle.
//     wyndex_setup.setup_pools(vec![]).unwrap();

//     // TODO: set balances for test users and env
//     wyndex_setup.set_balances(vec![]).unwrap();

//     let vault_token = Cw20Base::new(VAULT_TOKEN, mock.clone());
//     let cw20_id = vault_token.upload().unwrap().uploaded_code_id().unwrap();

//     let instantiate_msg = autocompounder::msg::AutocompounderInstantiateMsg {
//         code_id: Some(cw20_id),
//         commission_addr: common::COMMISSION_RECEIVER.to_string(),
//         deposit_fees: Decimal::percent(0),
//         dex: WYNDEX.to_string(),
//         performance_fees: Decimal::percent(3),
//         pool_assets: vault_pool.1.assets.clone(),
//         withdrawal_fees: Decimal::percent(0),
//         bonding_data: Some(BondingData {
//             unbonding_period: Duration::Time(1),
//             max_claims_per_address: None,
//         }),
//         max_swap_spread: Some(Decimal::percent(50)),
//     };

//     let vault = GenericVault::new(mock, wyndex_setup, &instantiate_msg).unwrap();

//     // TODO: Check autocompounder config
//     let _config: Config = vault.autocompounder_app.config().unwrap();

//     Ok(vault)
// }

// fn setup_mock_native_vault() -> Result<GenericVault<MockBech32, SetupWyndDex<MockBech32>>, AbstractInterfaceError> {
//     let mock = MockBech32::new(&common::OWNER);
//     let owner = mock.sender();
//     let wyndex_owner = mock.addr_make(WYNDEX_OWNER);
//     let _user1 = mock.addr_make(common::USER1);
//     let commission_receiver = mock.addr_make(common::COMMISSION_RECEIVER);

//     let _abstract_ = Abstract::deploy_on(mock.clone(), mock.sender().to_string())?;
//     let wyndex = WynDex::store_on(mock.clone()).unwrap();


//     let WynDex {
//         // eur_token,
//         // usd_token,
//         // wynd_token,
//         wynd_eur_lp,
//         wynd_eur_pair,
//         eur_usd_pair,
//         // eur_usd_lp,
//         eur_usd_staking,
//         ..
//     } = wyndex;

//     let pools = vec![
//         (
//             PoolAddressBase::contract(eur_usd_pair),
//             PoolMetadata::stable(WYNDEX, vec![EUR, USD]),
//         ),
//         (
//             PoolAddressBase::contract(wynd_eur_pair),
//             PoolMetadata::stable(WYNDEX, vec![WYND_TOKEN, EUR]),
//         ),
//     ];

//     let assets: Vec<AssetWithInfo> = vec![
//         (EUR.to_string(), AssetInfoBase::native(EUR)),
//         (USD.to_string(), AssetInfoBase::native(USD)),
//         (
//             WYND_TOKEN.to_string(),
//             AssetInfoBase::cw20(Addr::unchecked(WYND_TOKEN)),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![EUR, WYND_TOKEN]).to_string(),
//             AssetInfoBase::cw20(wynd_eur_lp.address()?),
//         ),
//         (
//             LpToken::new(WYNDEX, vec![EUR, USD]).to_string(),
//             AssetInfoBase::cw20(wynd_eur_lp.address()?),
//         ),
//     ]
//     .into_iter()
//     .map(|f| AssetWithInfo::new(f.0, f.1))
//     .collect();

//     let eur_usd_lp_asset = LpToken::new(WYNDEX, vec![EUR, USD]);
//     let contracts = vec![(
//         UncheckedContractEntry::new(WYNDEX.to_string(), format!("staking/{eur_usd_lp_asset}")),
//         eur_usd_staking.to_string(),
//     )];

//     let mut wyndex_setup = SetupWyndDex {
//         chain: mock.clone(),
//         dex_base: DexBase {
//             pools: pools.clone(),
//             contracts,
//             assets,
//             reward_tokens: vec![],
//         },
//         cw20_minter: wyndex_owner,
//         name: "wyndex".to_string(),
//     };

//     wyndex_setup.setup_pools(vec![]).unwrap();
//     wyndex_setup
//         .set_balances(vec![(
//             owner.to_string().as_str(),
//             vec![
//                 Asset::new(AssetInfo::native(USD), 10_000u128),
//                 Asset::new(AssetInfo::native(EUR), 10_000u128),
//             ],
//         )])
//         .unwrap();

//     let vault_token = Cw20Base::new(VAULT_TOKEN, mock.clone());
//     let cw20_id = vault_token.upload().unwrap().uploaded_code_id().unwrap();

//     let instantiate_msg = autocompounder::msg::AutocompounderInstantiateMsg {
//         code_id: Some(cw20_id),
//         commission_addr: commission_receiver.into_string(),
//         deposit_fees: Decimal::percent(0),
//         dex: WYNDEX.to_string(),
//         performance_fees: Decimal::percent(3),
//         pool_assets: pools.clone().first().unwrap().1.assets.clone(),
//         withdrawal_fees: Decimal::percent(0),
//         bonding_data: Some(BondingData {
//             unbonding_period: Duration::Time(1),
//             max_claims_per_address: None,
//         }),
//         max_swap_spread: Some(Decimal::percent(50)),
//     };

//     let vault = GenericVault::new(mock, wyndex_setup, &instantiate_msg).unwrap();

//     // TODO: Check autocompounder config
//     let _config: Config = vault.autocompounder_app.config().unwrap();

//     Ok(vault)
// }

// fn ans_info_from_osmosis_pools(
//     pools: &Vec<(PoolAddressBase<String>, PoolMetadata)>,
// ) -> Vec<(String, AssetInfo)> {
//     pools
//         .iter()
//         .map(|(pool_id, metadata)| {
//             let cs_assets = metadata
//                 .assets
//                 .iter()
//                 .map(|a| a.to_string())
//                 .collect::<Vec<String>>();

//             let pool_id = get_id_from_osmo_pool(pool_id);

//             (
//                 format!("{}/{}", metadata.dex, cs_assets.join(","),),
//                 AssetInfo::native(format!("gamm/pool/{pool_id}")),
//             )
//         })
//         .collect::<Vec<_>>()
// }



// #[test]
// #[ignore]
// fn deposit_assets_cw20_mock() -> AResult {
//     let vault = setup_mock_cw20_vault()?;
//     let owner = Addr::unchecked(common::OWNER);
//     let user1 = Addr::unchecked(common::USER1);
//     test_deposit_assets(vault, &owner, &owner, &user1, &user1)
// }

// #[test]
// #[ignore]
// fn deposit_assets_native_mock() -> AResult {
//     let vault = setup_mock_native_vault()?;
//     let owner = Addr::unchecked(common::OWNER);
//     let user1 = Addr::unchecked(common::USER1);
//     test_deposit_assets(vault, &owner, &owner, &user1, &user1)
// }


