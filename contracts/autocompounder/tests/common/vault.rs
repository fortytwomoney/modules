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

pub struct GenericDex {
    pub assets: Vec<(String, AssetInfoBase<Addr>)>,
    pub pools: Vec<(UncheckedPoolAddress, PoolMetadata)>,
    pub dex_name: String,
}

impl GenericDex {
    pub fn new(
        assets: Vec<(String, AssetInfoBase<Addr>)>,
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
                    AssetInfoBase::Cw20(c) => AssetInfo::cw20(c.clone()),
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



impl<T: MutCwEnv + Clone + 'static> GenericVault<T> {
    pub fn new(
        chain: T,
        assets: Vec<(String, AssetInfo)>,
        dex: GenericDex,
        autocompounder_instantiate_msg: &autocompounder::msg::AutocompounderInstantiateMsg,
    ) -> Result<Self, Error> {
        // Initialize the blockchain environment, similar to OsmosisTestTube setup
        let chain_env = chain.clone(); // Assuming T can be used similar to OsmosisTestTube

        // TODO: Add balance init for accounts. This should include both cw20 assets as native assets.
        let unchecked_assets = assets
            .iter()
            .map(|(symbol, asset_info)| (symbol.clone(), asset_info.into()))
            .collect();
        


        // Setup the abstract client similar to the provided `setup_vault` function
        let abstract_client = AbstractClient::builder(chain_env.clone())
            .assets(unchecked_assets)
            .dex(&dex.dex_name)
            .pools(dex.pools.clone())
            .build()?; // Simplified for illustration

        let (dex_adapter, staking_adapter, _fortytwo_publisher, account, autocompounder_app) =
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