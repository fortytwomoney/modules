use abstract_app::objects::pool_id::UncheckedPoolAddress;
use abstract_client::AbstractClient;
use abstract_client::{Account, Application};

use abstract_core::objects::{AnsAsset, PoolMetadata};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_interface::{Abstract, AbstractAccount};
use anyhow::Error;
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns, Config};
use cosmwasm_std::{coin, coins, Addr, Coin};
use cw20::msg::Cw20ExecuteMsgFns;
use cw20_base::msg::QueryMsgFns as _;
use cw_asset::{AssetInfo, AssetInfoBase};

use cw_orch::contract::interface_traits::CallAs;
use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::environment::{CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;

use cw_plus_interface::cw20_base::Cw20Base;
use wyndex_bundle::WynDex;

use super::account_setup::setup_autocompounder_account;
use super::dexes::DexInit;
use super::AResult;

#[derive(Clone, Debug)]
pub struct AssetWithInfo {
    pub ans_name: String,
    pub asset_info: AssetInfo,
}

impl AssetWithInfo {
    pub fn new<T: Into<String>, U: Into<AssetInfoBase<Addr>>>(name: T, info: U) -> Self {
        Self {
            ans_name: name.into(),
            asset_info: info.into(),
        }
    }
}

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
pub struct GenericVault<Chain: CwEnv, Dex: DexInit> {
    pub account: Account<Chain>,
    pub autocompounder_app: Application<Chain, AutocompounderApp<Chain>>,
    pub staking_adapter: CwStakingAdapter<Chain>,
    pub dex_adapter: DexAdapter<Chain>,
    pub dex: Dex,
    pub abstract_client: AbstractClient<Chain>,
    pub chain: Chain,
    pub signing_account: Option<SigningAccount>, // preferably this is not included in the struct, but needed to initially set balances for osmosis_testtube
}

#[allow(dead_code)]
impl<T: CwEnv, Dex: DexInit> GenericVault<T, Dex> {
    pub fn redeem_vault_token(
        &self,
        amount: u128,
        sender: &<T as TxHandler>::Sender,
        reciever: Option<Addr>,
    ) -> Result<<T as cw_orch::prelude::TxHandler>::Response, Error>
    where
        T: cw_orch::prelude::TxHandler,
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

    pub fn vault_token_balance<S: Into<String>>(&self, account: S) -> Result<u128, Error> {
        match self.autocompounder_app.config()?.vault_token {
            AssetInfoBase::Cw20(c) => {
                let vault_token = Cw20Base::new(c, self.chain.clone());
                Ok(vault_token.balance(account.into().clone())?.balance.u128())
            }
            // @Buckram123 HELP: how do i Properly handle the balance().unwrap()
            AssetInfoBase::Native(denom) => Ok(self
                .chain
                .balance(account.into(), Some(denom))
                .unwrap()
                .first()
                .unwrap()
                .amount
                .u128()),
            _ => panic!("invalid vault token"),
        }
    }

    fn asset_balance(&self, account: String, asset: u8) -> Result<u128, Error> {
        let dex_base = self.dex.dex_base();
        let asset = match asset {
            1 => dex_base.asset_a(),
            2 => dex_base.asset_b(),
            _ => panic!("invalid asset"),
        };
        match asset.asset_info.clone() {
            AssetInfoBase::Cw20(c) => {
                let cw20_asset = Cw20Base::new(c, self.chain.clone());
                Ok(cw20_asset.balance(account.clone())?.balance.u128())
            }
            AssetInfoBase::Native(denom) => Ok(self
                .chain
                .balance(account, Some(denom))
                .unwrap()
                .first()
                .unwrap()
                .amount
                .u128()),
            _ => panic!("invalid asset_info"),
        }
    }

    pub fn asset_a_balance(&self, account: String) -> Result<u128, Error> {
        self.asset_balance(account, 1)
    }

    pub fn asset_b_balance(&self, account: String) -> Result<u128, Error> {
        self.asset_balance(account, 2)
    }

    pub fn asset_balances<S: Into<String> + Clone>(&self, account: S) -> Result<(u128, u128), Error> {
        Ok((
            self.asset_a_balance(account.clone().into())?,
            self.asset_b_balance(account.into())?,
        ))
    }

    pub fn pending_claims(&self, account: &Addr) -> Result<u128, Error> {
        Ok(self.autocompounder_app.pending_claims(account.clone())?.into())
    }
}

#[allow(dead_code)]
impl<T: MutCwEnv + Clone + 'static, Dex: DexInit> GenericVault<T, Dex> {
    pub fn new(
        chain: T,
        dex: Dex,
        autocompounder_instantiate_msg: &autocompounder::msg::AutocompounderInstantiateMsg,
    ) -> Result<Self, Error> {
        // Initialize the blockchain environment, similar to OsmosisTestTube setup
        let chain_env = chain.clone(); // Assuming T can be used similar to OsmosisTestTube

        let dex_base = dex.dex_base();

        let unchecked_assets = dex_base
            .assets
            .iter()
            .map(|asset| (asset.ans_name.clone(), asset.asset_info.clone().into()))
            .collect();

        // Setup the abstract client similar to the provided `setup_vault` function
        let abstract_client = AbstractClient::builder(chain_env.clone())
            .assets(unchecked_assets)
            .dex(&dex.name())
            .pools(dex_base.pools.clone())
            .contracts(dex_base.contracts.clone())
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
            dex: dex,
            abstract_client,
            signing_account: None,
        })
    }
}

// Dex convenience functions
#[allow(dead_code)]
impl<Chain: CwEnv, Dex: DexInit> GenericVault<Chain, Dex> {
    fn main_pool(&self) -> (UncheckedPoolAddress, PoolMetadata) {
        self.dex.dex_base().pools.first().unwrap().clone()
    }

    /// Allows for depositing any amount without having to care about cw20 or native assets
    pub fn deposit_assets(
        &self,
        depositor: &Chain::Sender,
        amount_a: u128,
        amount_b: u128,
        recipient: Option<Addr>,
    ) -> AResult {
        let dex_base = self.dex.dex_base();

        let asset_a = dex_base.asset_a();
        let asset_b = dex_base.asset_b();

        let assets = vec![(&asset_a, amount_a), (&asset_b, amount_b)]
            .into_iter()
            .filter(|(_, amount)| *amount > 0)
            .collect::<Vec<_>>();

        println!("Depositing assets: {:?}", assets);

        let ans_assets_deposit_coins = assets
            .into_iter()
            .map(|(asset, amount)| self.asset_amount_to_deposit(depositor, amount, asset))
            .collect::<Result<Vec<(AnsAsset, Option<Coin>)>, Error>>()?;

        let (ans_assets, deposit_coins): (Vec<_>, Vec<_>) =
            ans_assets_deposit_coins.into_iter().unzip();

        let deposit_coins = deposit_coins
            .into_iter()
            .filter_map(|x| x)
            .collect::<Vec<_>>();

        println!(
            "Depositing coins: and assets {:?}, {:?}",
            ans_assets, deposit_coins
        );

        self.autocompounder_app.call_as(depositor).deposit(
            ans_assets,
            None,
            recipient,
            &deposit_coins,
        )?;

        Ok(())
    }

    fn asset_amount_to_deposit(
        &self,
        depositor: &Chain::Sender,
        amount: u128,
        asset: &AssetWithInfo,
    ) -> Result<(AnsAsset, Option<Coin>), Error> {
        match &asset.asset_info {
            AssetInfoBase::Cw20(addr) => {
                let cw20_asset = Cw20Base::new(addr, self.chain.clone());
                cw20_asset.call_as(depositor).increase_allowance(
                    amount.into(),
                    self.autocompounder_app.addr_str()?,
                    None,
                )?;
                Ok((AnsAsset::new(asset.ans_name.clone(), amount), None))
            }
            AssetInfoBase::Native(denom) => Ok((
                AnsAsset::new(asset.ans_name.clone(), amount),
                Some(coin(amount, denom)),
            )),
            _ => panic!("invalid asset_info"),
        }
    }
}

// NOTE: I think because Osmosis has only native assets, and Astroport has both, we should have 3 environments:
// - Osmosis environment with only native asset pools
// - Astroport environment with only cw20 asset pools
// - Astroport environment with both native and cw20 pools
//
// in this way, we can run the same tests for cw20 and native pools.?
