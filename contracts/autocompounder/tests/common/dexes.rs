use std::rc::Rc;

use abstract_app::objects::UncheckedContractEntry;
use abstract_core::objects::pool_id::{PoolAddressBase, UncheckedPoolAddress};
use abstract_core::objects::{AssetEntry, PoolMetadata};
use cosmwasm_std::{coin, Addr, Coin};
use cw20::msg::Cw20ExecuteMsgFns;
use cw_asset::{Asset, AssetInfo};
use cw_orch::contract::interface_traits::CallAs;
use cw_orch::daemon::networks::osmosis;
use cw_orch::environment::{CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;
use cw_orch::osmosis_test_tube::OsmosisTestTube;
use cw_plus_interface::cw20_base::Cw20Base;
use osmosis_std::cosmwasm_to_proto_coins;
use osmosis_std::shim::Timestamp;
use osmosis_std::types::osmosis::incentives::MsgCreateGauge;
use osmosis_test_tube::Module;

use super::osmosis_pool_incentives_module::Incentives;
use super::vault::AssetWithInfo;

pub struct IncentiveParams {
    /// start time in seconds would default to the current block time if none
    start_time: Option<u64>,
    num_epochs_paid_over: u64,
    coins: Vec<cosmwasm_std::Coin>,
}

#[allow(dead_code)]
impl IncentiveParams {
    pub fn new(coins: Vec<cosmwasm_std::Coin>, num_epochs: u64) -> IncentiveParams {
        IncentiveParams {
            coins,
            start_time: None,
            num_epochs_paid_over: num_epochs,
        }
    }

    pub fn from_coin<T: Into<String>>(amount: u128, denom: T, num_epochs: u64) -> IncentiveParams {
        IncentiveParams {
            coins: vec![coin(amount, denom.into())],
            start_time: None,
            num_epochs_paid_over: num_epochs,
        }
    }
}

#[derive(Clone, Default)]
pub struct DexBase {
    pub assets: Vec<AssetWithInfo>,
    pub pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
    pub contracts: Vec<(UncheckedContractEntry, String)>,
}

///
pub trait DexInit {
    fn set_balances(
        &self,
        balances: Vec<(&str, Vec<Asset>)>,
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn setup_pools(
        &self,
        initial_liquidity: Vec<Vec<Asset>>,
    ) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>;
    fn setup_incentives(
        &self,
        pool: &(PoolAddressBase<String>, PoolMetadata),
        incentives: IncentiveParams,
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn setup_dex<Chain: CwEnv>(
        ans_references: Vec<(String, AssetInfo)>,
        initial_liquidity: Vec<Vec<Asset>>,
        initial_balances: Vec<(&str, Vec<Asset>)>,
        incentives: IncentiveParams,
    ) -> Result<(Self, Chain), Box<dyn std::error::Error>>
    where
        Self: Sized;

    fn dex_base(&self) -> DexBase;
    fn name(&self) -> &str;
}

pub struct OsmosisDex<OsmosisTestTube>  {
    pub chain: OsmosisTestTube,
    pub signer: Rc<SigningAccount>,
    pub name: String,
    pub dex_base: DexBase,
    pub accounts: Vec<Rc<SigningAccount>>,
}

impl OsmosisDex<OsmosisTestTube> {
    fn new(chain: OsmosisTestTube) -> Self {
        Self {
            chain,
            signer: chain.sender,
            accounts: vec![],
            dex_base: DexBase::default(),
            name: "osmosis".to_string(),
        }
    }
}

impl DexInit for OsmosisDex<OsmosisTestTube> {

    fn name(&self) -> &str {
        &self.name
    }

    fn setup_dex<Chain: CwEnv>(
        ans_references: Vec<(String, AssetInfo)>,
        initial_liquidity: Vec<Vec<Asset>>,
        initial_balances: Vec<(&str, Vec<Asset>)>,
        incentives: IncentiveParams,
    ) -> Result<(Self, Chain), Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        let initial_coins = ans_references
            .iter()
            .map(|(denom, _)| coin(1_000_000_000_000, denom))
            .collect::<Vec<Coin>>();
        let mut chain = OsmosisTestTube::new(initial_coins);

        let mut osmosis = OsmosisDex::new(chain);

        osmosis.set_balances(initial_balances)?;

        let pools = osmosis.setup_pools(initial_liquidity)?;
        let main_pool = pools.first().unwrap();

        osmosis.setup_incentives(main_pool, incentives)?;

        let gamm_tokens = ans_info_from_osmosis_pools(&pools);

        let assets = vec![ans_references, gamm_tokens]
            .concat()
            .iter()
            .map(|(ans_name, asset_info)| AssetWithInfo::new(ans_name, asset_info.clone()))
            .collect::<Vec<AssetWithInfo>>();

        osmosis.dex_base.assets = assets;

        Ok((osmosis, chain))
    }

    fn set_balances(
        &self,
        balances: Vec<(&str, Vec<Asset>)>,
    ) -> Result<(), Box<(dyn std::error::Error + 'static)>> {
        let accounts: Vec<Rc<SigningAccount>> = balances
            .into_iter()
            .map(
                |(address, assets)| -> Result<_, Box<dyn std::error::Error>> {
                    let (native_balances, cw20_balances) =
                        split_native_from_cw20_assets(assets.to_vec());

                    if !cw20_balances.is_empty() {
                        panic!("This method is only for setting native assets, no cw20");
                    }

                    let account = self.chain.init_account(native_balances)?;

                    Ok(account)
                },
            )
            .collect::<Result<_, Box<dyn std::error::Error>>>()?;

        self.accounts = accounts;
        Ok(())
    }

    fn setup_pools(
        &self,
        initial_liquidity: Vec<Vec<Asset>>,
    ) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>> {
        let pools = initial_liquidity
            .iter()
            .map(|liquidity| -> Result<(UncheckedPoolAddress, PoolMetadata), Box<dyn std::error::Error>> {
                // map all assets as native. if not raise error
                let native_liquidity = liquidity.into_iter().map(|asset| -> Result<Coin, Box<dyn std::error::Error>> {
                    match asset.info.clone() {
                        AssetInfo::Native(denom) => {
                            Ok(coin(asset.amount.into(), denom.clone()))
                        },
                        _ => panic!("This method is for setting up native assets, not cw20")
                    }
                }).collect::<Result<Vec<_>, _>>()?;
                let pool_id = self.chain.create_pool(native_liquidity.clone()).map_err(Box::new)?;


                Ok((
                    PoolAddressBase::id(pool_id),
                    PoolMetadata::constant_product(self.name.clone(), native_liquidity.iter().map(|c| c.denom.clone()).collect::<Vec<String>>())
                ))
            })
            .collect::<Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>>();

        let pools = pools?;
        Ok(pools)
    }

    fn setup_incentives(
        &self,
        pool: &(PoolAddressBase<String>, PoolMetadata),
        incentives: IncentiveParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let osmosistesttube = &*self.chain.app.borrow();
        let incentive_wrapper = Incentives::new(osmosistesttube);

        incentive_wrapper
            .create_gauge(
                MsgCreateGauge {
                    is_perpetual: false,
                    owner: self.chain.sender().to_string(),
                    distribute_to: None,
                    coins: cosmwasm_to_proto_coins(incentives.coins),
                    start_time: match incentives.start_time {
                        Some(val) => Some(Timestamp {
                            seconds: i64::try_from(val).map_err(|_e| {
                                cosmwasm_std::StdError::generic_err("try from u64 to i64 error")
                            })?,
                            nanos: 0,
                        }),
                        None => None,
                    },
                    num_epochs_paid_over: incentives.num_epochs_paid_over,
                    pool_id: get_id_from_osmo_pool(&pool.0),
                },
                &self.chain.sender,
            )
            .map_err(|e| {
                Box::new(cosmwasm_std::StdError::generic_err(e.to_string()))
                    as Box<dyn std::error::Error>
            })?;

        Ok(())
    }

    fn dex_base(&self) -> DexBase {
        self.dex_base.clone()
    }
}

pub struct WyndDex<Chain: CwEnv> {
    pub chain: Chain,
    pub cw20_minter: <Chain as TxHandler>::Sender,
    pub dex_base: DexBase,
    pub name: String,
}

impl<Chain: MutCwEnv> DexInit for WyndDex<Chain> {
    fn set_balances(
        &self,
        balances: Vec<(&str, Vec<Asset>)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        balances.into_iter().try_for_each(
            |(address, assets)| -> Result<(), Box<dyn std::error::Error>> {
                let (native_balances, cw20_balances) =
                    split_native_from_cw20_assets(assets.to_vec());

                self.chain
                    .set_balance(&Addr::unchecked(address), native_balances)
                    .map_err(|err: <Chain as TxHandler>::Error| Box::new(err.into()))?;

                let _ = cw20_balances.into_iter().try_for_each(
                    |(cw20, amount)| -> Result<(), Box<dyn std::error::Error>> {
                        let cw20_token = Cw20Base::new(cw20, self.chain.clone());
                        let _res = cw20_token
                            .call_as(&self.cw20_minter)
                            .mint(amount.into(), address.to_string())
                            .map_err(|err: cw_orch::prelude::CwOrchError| {
                                Box::new(err) as Box<dyn std::error::Error>
                            })?;
                        Ok(())
                    },
                );

                Ok(()) // https://github.com/fortytwomoney/modules/blob/e82f2570cfb3c3ca88b8cc005db26a940538592e/contracts/autocompounder/tests/common/vault.rs#L49-L156)
            },
        );
        Ok(())
    }

    fn setup_pools(
        &self,
        initial_liquidity: Vec<Vec<Asset>>,
    ) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>> {
        // my current idea is to just verify whether the pools with the initial liquidity actuaally exist on chain. if not raise an error
        // TODO: verify whether pools exist on chain

        // TODO: LATER: really create the pools here for wyndex (maybe this is not worth it and instead we should do astroport)

        let pools = initial_liquidity.iter().map(|liquidity| -> Result<(UncheckedPoolAddress, PoolMetadata), Box<dyn std::error::Error>>{
            let pool_base = PoolAddressBase::contract("astroport_pool_address".to_string());
            let pool_metadata = PoolMetadata::constant_product(self.name.clone(), liquidity.iter().map(|c| {
                c.info.to_string()
            }).collect::<Vec<String>>());
            Ok(( pool_base, pool_metadata ))
        }).collect::<Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>>()?;

        Ok(pools)
    }

    fn setup_incentives(
        &self,
        pool: &(PoolAddressBase<String>, PoolMetadata),
        incentives: IncentiveParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
    
    fn dex_base(&self) -> DexBase {
        self.dex_base.clone()
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn setup_dex<C: CwEnv>(
        ans_references: Vec<(String, AssetInfo)>,
        initial_liquidity: Vec<Vec<Asset>>,
        initial_balances: Vec<(&str, Vec<Asset>)>,
        incentives: IncentiveParams,
    ) -> Result<(Self, C), Box<dyn std::error::Error>>
    where
        Self: Sized {
        todo!()
    }
    
}

/// UTILS
fn split_native_from_cw20_assets(
    assets: Vec<cw_asset::AssetBase<Addr>>,
) -> (Vec<cosmwasm_std::Coin>, Vec<(cosmwasm_std::Addr, u128)>) {
    assets.iter().fold((vec![], vec![]), |mut res, asset| {
        match &asset.info {
            AssetInfo::Cw20(c) => {
                res.1.push((c.clone(), asset.amount.into()));
            }
            AssetInfo::Native(n) => {
                let fund = cosmwasm_std::coin(asset.amount.into(), n.clone());
                res.0.push(fund);
            }
            _ => {}
        }
        res
    })
}

pub fn get_id_from_osmo_pool(pool_id: &PoolAddressBase<String>) -> u64 {
    match pool_id {
        PoolAddressBase::Id(id) => *id,
        _ => panic!("Invalid pool ID"),
    }
}

fn ans_info_from_osmosis_pools(
    pools: &Vec<(PoolAddressBase<String>, PoolMetadata)>,
) -> Vec<(String, AssetInfo)> {
    pools
        .iter()
        .map(|(pool_id, metadata)| {
            let cs_assets = metadata
                .assets
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<String>>();

            let pool_id = get_id_from_osmo_pool(pool_id);

            (
                format!("{}/{}", metadata.dex, cs_assets.join(","),),
                AssetInfo::native(format!("gamm/pool/{pool_id}")),
            )
        })
        .collect::<Vec<_>>()
}
