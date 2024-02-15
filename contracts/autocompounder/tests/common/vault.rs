use std::rc::Rc;

use abstract_client::AbstractClient;
use abstract_client::{Account, Application};
use abstract_core::objects::pool_id::{PoolAddressBase, UncheckedPoolAddress};
use abstract_core::objects::{AssetEntry, PoolMetadata, PoolType};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_interface::{Abstract, AbstractAccount};
use anyhow::Error;
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};
use cosmwasm_std::{coin, coins, Addr, Coin};
use cw20::msg::Cw20ExecuteMsgFns;
use cw_asset::{Asset, AssetInfo, AssetInfoBase, AssetInfoUnchecked};
use cw_orch::contract::interface_traits::CallAs;
use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::environment::{CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;
use cw_orch::osmosis_test_tube::OsmosisTestTube;
use cw_plus_interface::cw20_base::Cw20Base;
use wyndex_bundle::WynDex;

use super::account_setup::setup_autocompounder_account;
use super::COMMISSION_RECEIVER;

#[allow(dead_code)]
pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20Base<Chain>,
    pub staking: CwStakingAdapter<Chain>,
    pub dex: DexAdapter<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}

#[allow(dead_code)]
pub struct GenericVault<Chain: CwEnv> {
    pub account: Account<Chain>,
    pub autocompounder_app: Application<Chain, AutocompounderApp<Chain>>,
    pub staking_adapter: CwStakingAdapter<Chain>,
    pub dex_adapter: DexAdapter<Chain>,
    pub dex: GenericDex,
    pub abstract_client: AbstractClient<Chain>,
    pub chain: Chain,
    pub signing_account: Option<SigningAccount>, // preferably this is not included in the struct, but needed to initially set balances for osmosis_testtube
}

pub struct AstroportDex<Chain: CwEnv> {
    pub chain: Chain,
    pub cw20_minter: <Chain as TxHandler>::Sender,
    pub assets: Vec<AssetInfo>,
    pub pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
    pub name: String,
}

pub struct OsmosisDex<OsmosisTestTube> {
    pub chain: OsmosisTestTube,
    pub signer: Rc<SigningAccount>,
    pub assets: Vec<String>,
    pub pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
    pub name: String,
}

trait DexInit {
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

impl<Chain: MutCwEnv> DexInit for AstroportDex<Chain> {
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

pub struct GenericDex {
    pub assets: Vec<(String, AssetInfoBase<String>)>,
    pub pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
    pub dex_name: String,
}

impl GenericDex {
    pub fn new(
        assets: Vec<(String, AssetInfoBase<String>)>,
        pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
        dex_name: String,
    ) -> Self {
        Self {
            assets,
            pools,
            dex_name,
        }
    }

    pub fn asset_entries(&self) -> Vec<AssetEntry> {
        self.assets
            .iter()
            .map(|asset| {
                let (symbol, _asset_info) = asset;
                AssetEntry::new(symbol.as_str())
            })
            .collect()
    }

    pub fn asset_infos(&self) -> Vec<AssetInfo> {
        self.assets
            .iter()
            .map(|asset| {
                let (_, asset_info) = asset;
                match asset_info {
                    AssetInfoBase::Cw20(c) => AssetInfo::cw20(Addr::unchecked(c.clone())),
                    AssetInfoBase::Native(denom) => AssetInfo::native(denom.clone()),
                    _ => panic!("invalid base"),
                }
            })
            .collect()
    }

    /// returns the first pool. should be the main pool
    pub fn main_pool(&self) -> (UncheckedPoolAddress, PoolMetadata) {
        self.pools.first().unwrap().to_owned()
    }
}

impl<T: CwEnv> GenericVault<T> {
    pub fn redeem_vault_token(
        &self,
        amount: u128,
        sender: &Addr,
        reciever: Option<Addr>,
    ) -> Result<<T as cw_orch::prelude::TxHandler>::Response, Error>
    where
        T: cw_orch::prelude::TxHandler<Sender = Addr>,
    {
        let config = self.autocompounder_app.config()?;
        match config.vault_token {
            AssetInfoBase::Cw20(c) => {
                let vault_token = Cw20Base::new(c, self.chain.clone());
                let _res = vault_token.call_as(sender).increase_allowance(
                    amount.into(),
                    self.autocompounder_app.addr_str()?,
                    None,
                )?;

                Ok(self
                    .autocompounder_app
                    .call_as(sender)
                    .redeem(amount.into(), reciever, &[])?)
            }
            AssetInfoBase::Native(denom) => {
                let res = self.autocompounder_app.call_as(sender).redeem(
                    amount.into(),
                    reciever,
                    &coins(amount, denom),
                )?;
                Ok(res)
            }
            _ => panic!("invalid vault token"),
        }
    }
}

type StartingBalances<'a> = Vec<(&'a Addr, &'a Vec<Asset>)>;
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

impl<T: MutCwEnv + Clone + 'static> GenericVault<T> {
    pub fn new(
        chain: T,
        assets: Vec<(String, AssetInfoUnchecked)>,
        dex: GenericDex,
        autocompounder_instantiate_msg: &autocompounder::msg::AutocompounderInstantiateMsg,
    ) -> Result<Self, Error> {
        // Initialize the blockchain environment, similar to OsmosisTestTube setup
        let chain_env = chain.clone(); // Assuming T can be used similar to OsmosisTestTube

        // TODO: Add balance init for accounts. This should include both cw20 assets as native assets.

        // Setup the abstract client similar to the provided `setup_vault` function
        let abstract_client = AbstractClient::builder(chain_env.clone())
            .assets(assets)
            .dex(&dex.dex_name)
            .pools(dex.pools.clone())
            .build()?; // Simplified for illustration

        let (dex_adapter, staking_adapter, fortytwo_publisher, account, autocompounder_app) =
            setup_autocompounder_account(&abstract_client, &autocompounder_instantiate_msg)?;

        // Return the constructed GenericVault instance
        Ok(Self {
            chain: chain_env,
            account,
            autocompounder_app,
            dex_adapter,
            staking_adapter,
            dex,
            abstract_client,
            signing_account: None,
        })
    }
}

// NOTE: I think because Osmosis has only native assets, and Astroport has both, we should have 3 environments: 
// - Osmosis environment with only native asset pools
// - Astroport environment with only cw20 asset pools
// - Astroport environment with both native and cw20 pools
//
// in this way, we can run the same tests for cw20 and native pools.?