use std::rc::Rc;

use abstract_app::objects::{LpToken, UncheckedContractEntry};
use abstract_core::objects::pool_id::{PoolAddressBase, UncheckedPoolAddress};
use abstract_core::objects::PoolMetadata;
use cosmwasm_std::{coin, Addr, Coin};
use cw20::msg::Cw20ExecuteMsgFns;
use cw_asset::{Asset, AssetInfo};
use cw_orch::contract::interface_traits::CallAs;

use cw_orch::environment::{CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;
use cw_orch::osmosis_test_tube::OsmosisTestTube;
use cw_plus_interface::cw20_base::Cw20Base;
use osmosis_std::cosmwasm_to_proto_coins;
use osmosis_std::shim::{Duration, Timestamp};
use osmosis_std::types::osmosis::incentives::MsgCreateGauge;
use osmosis_std::types::osmosis::lockup::{LockQueryType, QueryCondition};
use osmosis_test_tube::Module;

use super::osmosis_pool_incentives_module::Incentives;
use super::vault::AssetWithInfo;

pub const OSMOSIS_EPOCH: u64 = 86400;
#[derive(Clone)]
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
    pub reward_tokens: Vec<AssetInfo>,
}

impl DexBase {
    pub fn main_pool(&self) -> (UncheckedPoolAddress, PoolMetadata) {
        self.pools.first().unwrap().clone()
    }

    pub fn asset_a(&self) -> &AssetWithInfo {
        self.assets.get(0).unwrap()
    }

    pub fn asset_b(&self) -> &AssetWithInfo {
        self.assets.get(1).unwrap()
    }
}

///
///
/// The `DexInit` trait provides the necessary methods for setting up a decentralized exchange (DEX).
///
pub trait DexInit {
    /// Sets the balances for the DEX.
    ///
    /// # Arguments
    ///
    /// * `balances` - A vector of tuples, where each tuple contains a string reference and a vector of `Asset` objects.
    ///
    fn set_balances(
        &mut self,
        balances: Vec<(&str, Vec<Asset>)>,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Sets up the pools for the DEX.
    ///
    /// # Arguments
    ///
    /// * `initial_liquidity` - A vector of vectors, where each inner vector contains `Asset` objects.
    ///
    fn setup_pools(
        &self,
        initial_liquidity: Vec<Vec<Asset>>,
    ) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>;

    /// Sets up the incentives for the DEX.
    ///
    /// # Arguments
    ///
    /// * `pool` - A tuple containing a `PoolAddressBase<String>` and a `PoolMetadata`.
    /// * `incentives` - An `IncentiveParams` object.
    ///
    fn setup_incentives(
        &self,
        pool: &(PoolAddressBase<String>, PoolMetadata),
        incentives: IncentiveParams,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Sets up the DEX.
    ///
    /// # Arguments
    ///
    /// * `ans_references` - A vector of tuples, where each tuple contains a string and an `AssetInfo` object.
    /// * `initial_liquidity` - A vector of vectors, where each inner vector contains `Asset` objects.
    /// * `initial_balances` - A vector of tuples, where each tuple contains a string reference and a vector of `Asset` objects.
    /// * `incentives` - An `IncentiveParams` object.
    ///
    fn setup_dex<Chain: CwEnv>(
        ans_references: Vec<(String, AssetInfo)>,
        initial_liquidity: Vec<Vec<Asset>>,
        initial_balances: Vec<(&str, Vec<Asset>)>,
        incentives: IncentiveParams,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;

    /// Returns the base of the DEX.
    fn dex_base(&self) -> DexBase;

    /// Returns the LpToken of the dex.
    fn lp_token(&self,) -> LpToken;

    /// returns the asset info of the LP token
    fn lp_asset(&self) -> AssetInfo;

    /// Returns the name of the DEX.
    fn name(&self) -> &str;
}

pub struct OsmosisDex<OsmosisTestTube> {
    pub chain: OsmosisTestTube,
    pub signer: Rc<SigningAccount>,
    pub name: String,
    pub dex_base: DexBase,
    pub accounts: Vec<Rc<SigningAccount>>,
}

impl OsmosisDex<OsmosisTestTube> {
    /// Creates a new `OsmosisDex`.
    ///
    /// # Arguments
    ///
    /// * `chain` - An `OsmosisTestTube` object.
    ///
    fn new(chain: OsmosisTestTube) -> Self {
        Self {
            chain: chain.clone(),
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
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        let initial_coins = ans_references
            .iter()
            .map(|(denom, _)| coin(1_000_000_000_000, denom))
            .collect::<Vec<Coin>>();
        let chain = OsmosisTestTube::new(initial_coins);

        let mut osmosis = OsmosisDex::new(chain);

        osmosis.set_balances(initial_balances)?;

        let pools = osmosis.setup_pools(initial_liquidity)?;
        osmosis.dex_base.pools = pools.clone();
        let main_pool = pools.first().unwrap();

        osmosis.setup_incentives(main_pool, incentives.clone())?;

        osmosis.dex_base.reward_tokens = incentives
            .coins
            .iter()
            .map(|c| AssetInfo::native(c.denom.clone()))
            .collect::<Vec<AssetInfo>>();

        let gamm_tokens = ans_info_from_osmosis_pools(&pools);

        let assets = vec![ans_references, gamm_tokens]
            .concat()
            .iter()
            .map(|(ans_name, asset_info)| AssetWithInfo::new(ans_name, asset_info.clone()))
            .collect::<Vec<AssetWithInfo>>();

        osmosis.dex_base.assets = assets;

        Ok(osmosis)
    }

    fn set_balances(
        &mut self,
        balances: Vec<(&str, Vec<Asset>)>,
    ) -> Result<(), Box<(dyn std::error::Error + 'static)>> {
        let accounts: Vec<Rc<SigningAccount>> = balances
            .into_iter()
            .map(
                |(_address, assets)| -> Result<_, Box<dyn std::error::Error>> {
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

        let current_block = osmosistesttube.get_block_timestamp();
        let incentive_wrapper = Incentives::new(osmosistesttube);

        let pool_denom = format!("gamm/pool/{}", get_id_from_osmo_pool(&pool.0));

        let lock_query_condition = QueryCondition {
            lock_query_type: LockQueryType::ByDuration as i32,
            duration: Some(Duration {
                seconds: 1 as i64,
                nanos: 0,
            }),
            denom: pool_denom,
            timestamp: None,
        };

        // lockable_durations:
        //         Duration {
        //             seconds: 1,
        //             nanos: 0,
        //         },
        //         Duration {
        //             seconds: 3600,
        //             nanos: 0,
        //         },
        //         Duration {
        //             seconds: 10800,
        //             nanos: 0,
        //         },
        //         Duration {
        //             seconds: 25200,
        //             nanos: 0,
        //         },
        //     ],
        // }

        incentive_wrapper
            .create_gauge(
                MsgCreateGauge {
                    is_perpetual: false,
                    owner: self.chain.sender().to_string(),
                    distribute_to: Some(lock_query_condition),
                    coins: cosmwasm_to_proto_coins(incentives.coins),
                    start_time: match incentives.start_time {
                        Some(val) => Some(Timestamp {
                            seconds: val as i64,
                            nanos: 0,
                        }),
                        None => Some(Timestamp {
                            seconds: current_block.seconds() as i64,
                            nanos: 0,
                        }),
                    },
                    num_epochs_paid_over: incentives.num_epochs_paid_over,
                    pool_id: 0,
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

    fn lp_asset(&self,) -> AssetInfo {

        let ans_info = ans_info_from_osmosis_pools(&vec![self.dex_base.pools[0].clone()]);
        ans_info[0].1.clone()
    }

    fn lp_token(&self,) -> LpToken {
        LpToken {
            dex: self.name.clone(),
            assets: self.dex_base.pools[0].1.assets.clone(),
        }
    
    }
}

// pub struct WyndDex<Chain: CwEnv> {
//     pub chain: Chain,
//     pub cw20_minter: <Chain as TxHandler>::Sender,
//     pub dex_base: DexBase,
//     pub name: String,
// }

// impl<Chain: MutCwEnv> DexInit for WyndDex<Chain> {
//     fn set_balances(
//         &mut self,
//         balances: Vec<(&str, Vec<Asset>)>,
//     ) -> Result<(), Box<dyn std::error::Error>> {
//         let _ = balances.into_iter().try_for_each(
//             |(address, assets)| -> Result<(), Box<dyn std::error::Error>> {
//                 let (native_balances, cw20_balances) =
//                     split_native_from_cw20_assets(assets.to_vec());

//                 self.chain
//                     .set_balance(&Addr::unchecked(address), native_balances)
//                     .map_err(|err: <Chain as TxHandler>::Error| Box::new(err.into()))?;

//                 let _ = cw20_balances.into_iter().try_for_each(
//                     |(cw20, amount)| -> Result<(), Box<dyn std::error::Error>> {
//                         let cw20_token = Cw20Base::new(cw20, self.chain.clone());
//                         let _res = cw20_token
//                             .call_as(&self.cw20_minter)
//                             .mint(amount.into(), address.to_string())
//                             .map_err(|err: cw_orch::prelude::CwOrchError| {
//                                 Box::new(err) as Box<dyn std::error::Error>
//                             })?;
//                         Ok(())
//                     },
//                 );

//                 Ok(()) // https://github.com/fortytwomoney/modules/blob/e82f2570cfb3c3ca88b8cc005db26a940538592e/contracts/autocompounder/tests/common/vault.rs#L49-L156)
//             },
//         )?;
//         Ok(())
//     }

//     fn setup_pools(
//         &self,
//         initial_liquidity: Vec<Vec<Asset>>,
//     ) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>> {
//         // my current idea is to just verify whether the pools with the initial liquidity actuaally exist on chain. if not raise an error
//         // TODO: verify whether pools exist on chain

//         // TODO: LATER: really create the pools here for wyndex (maybe this is not worth it and instead we should do astroport)

//         let pools = initial_liquidity.iter().map(|liquidity| -> Result<(UncheckedPoolAddress, PoolMetadata), Box<dyn std::error::Error>>{
//             let pool_base = PoolAddressBase::contract("astroport_pool_address".to_string());
//             let pool_metadata = PoolMetadata::constant_product(self.name.clone(), liquidity.iter().map(|c| {
//                 c.info.to_string()
//             }).collect::<Vec<String>>());
//             Ok(( pool_base, pool_metadata ))
//         }).collect::<Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>>()?;

//         Ok(pools)
//     }

//     fn setup_incentives(
//         &self,
//         _pool: &(PoolAddressBase<String>, PoolMetadata),
//         _incentives: IncentiveParams,
//     ) -> Result<(), Box<dyn std::error::Error>> {
//         todo!()
//     }

//     fn dex_base(&self) -> DexBase {
//         self.dex_base.clone()
//     }

//     fn name(&self) -> &str {
//         &self.name
//     }

//     fn setup_dex<C: CwEnv>(
//         _ans_references: Vec<(String, AssetInfo)>,
//         _initial_liquidity: Vec<Vec<Asset>>,
//         _initial_balances: Vec<(&str, Vec<Asset>)>,
//         _incentives: IncentiveParams,
//     ) -> Result<Self, Box<dyn std::error::Error>>
//     where
//         Self: Sized,
//     {
//         todo!()
//     }
// }

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
