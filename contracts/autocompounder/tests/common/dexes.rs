use std::rc::Rc;

use abstract_core::objects::pool_id::{PoolAddressBase, UncheckedPoolAddress};
use abstract_core::objects::{AssetEntry, PoolMetadata};
use cosmwasm_std::{coin, Addr, Coin};
use cw20::msg::Cw20ExecuteMsgFns;
use cw_asset::{Asset, AssetInfo};
use cw_orch::contract::interface_traits::CallAs;
use cw_orch::environment::{CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;
use cw_orch::osmosis_test_tube::OsmosisTestTube;
use cw_plus_interface::cw20_base::Cw20Base;

pub struct WyndDex<Chain: CwEnv> {
    pub chain: Chain,
    pub cw20_minter: <Chain as TxHandler>::Sender,
    pub assets: Vec<AssetInfo>,
    pub name: String,
}

pub struct OsmosisDex<OsmosisTestTube> {
    pub chain: OsmosisTestTube,
    pub signer: Rc<SigningAccount>,
    pub assets: Vec<String>,
    pub name: String,
}

pub trait DexInit {
    fn base_assets(&self) -> Vec<AssetEntry>;
    fn asset_infos(&self) -> Vec<AssetInfo>;
    fn set_balances(
        &mut self,
        balances: &[(&Addr, &[Asset])],
    ) -> Result<(), Box<dyn std::error::Error>>;
    fn setup_pools(&self, initial_liquidity: Vec<Vec<Asset>>) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>;
}

impl DexInit for OsmosisDex<OsmosisTestTube> {
    fn base_assets(&self) -> Vec<AssetEntry> {
        self.assets
            .iter()
            .map(|asset| AssetEntry::new(asset.as_str()))
            .collect()
    }

    fn asset_infos(&self) -> Vec<AssetInfo> {
        self.assets
            .iter()
            .map(|asset| AssetInfo::cw20(Addr::unchecked(asset.clone())))
            .collect()
    }

    fn set_balances(
        &mut self,
        balances: &[(&Addr, &[Asset])],
    ) -> Result<(), Box<dyn std::error::Error>> {
        balances.into_iter().try_for_each(
            |(address, assets)| -> Result<(), Box<dyn std::error::Error>> {
                let (native_balances, cw20_balances) =
                    split_native_from_cw20_assets(assets.to_vec());

                if !cw20_balances.is_empty() {
                    panic!("This method is only for setting native assets, no cw20");
                }

                let _res = self
                    .chain
                    .call_as(&self.signer)
                    .bank_send(address.to_string(), native_balances)
                    .map_err(|err| Box::new(err) as Box<dyn std::error::Error>)?;

                Ok(())
            },
        )?;
        Ok(())
    }

    fn setup_pools(&self, initial_liquidity: Vec<Vec<Asset>>) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>> {
        initial_liquidity
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
            .collect::<Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>>()
    }
}

impl<Chain: MutCwEnv> DexInit for WyndDex<Chain> {
    fn base_assets(&self) -> Vec<AssetEntry> {
        self.assets
            .iter()
            .map(|asset| AssetEntry::new(asset.to_string().as_str()))
            .collect()
    }

    fn asset_infos(&self) -> Vec<AssetInfo> {
        todo!()
        // self.assets
        //     .iter()
        //     .map(|asset| *asset)
        //     .collect()
    }

    fn set_balances(
        &mut self,
        balances: &[(&Addr, &[Asset])],
    ) -> Result<(), Box<dyn std::error::Error>> {
        balances.into_iter().try_for_each(
            |(address, assets)| -> Result<(), Box<dyn std::error::Error>> {
                let (native_balances, cw20_balances) =
                    split_native_from_cw20_assets(assets.to_vec());

                self.chain
                    .set_balance(address, native_balances)
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
        )?;
        Ok(())
    }

    fn setup_pools(&self, initial_liquidity: Vec<Vec<Asset>>) -> Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>> {
        // my current idea is to just verify whether the pools with the initial liquidity actuaally exist on chain. if not raise an error
        // TODO: verify whether pools exist on chain

        // TODO: LATER: really create the pools here for astroport!

        let pools = initial_liquidity.iter().map(|liquidity| -> Result<(UncheckedPoolAddress, PoolMetadata), Box<dyn std::error::Error>>{
            let pool_base = PoolAddressBase::contract("astroport_pool_address".to_string());
            let pool_metadata = PoolMetadata::constant_product(self.name.clone(), liquidity.iter().map(|c| {
                c.info.to_string()
            }).collect::<Vec<String>>());
            Ok(( pool_base, pool_metadata ))
        }).collect::<Result<Vec<(UncheckedPoolAddress, PoolMetadata)>, Box<dyn std::error::Error>>>()?;

        Ok(pools)
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