use core::panic;

use abstract_app::objects::pool_id::UncheckedPoolAddress;
use abstract_app::objects::{AnsEntryConvertor, AssetEntry, LpToken};
use abstract_client::AbstractClient;
use abstract_client::{Account, Application};

use abstract_core::objects::{AnsAsset, PoolMetadata};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::api::DexInterface;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_dex_adapter::msg::{DexExecuteMsg, DexQueryMsgFns};
use abstract_dex_standard::ans_action::DexAnsAction;
use abstract_interface::{Abstract, AbstractAccount};

use anyhow::{Error, Ok};
use autocompounder::{convert_to_assets, convert_to_shares};
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns, Claim, Config};
use cosmwasm_std::{coin, coins, Addr, Coin, Uint128};
use cw20::msg::Cw20ExecuteMsgFns;
use cw20_base::msg::QueryMsgFns as _;
use cw_asset::{Asset, AssetInfo, AssetInfoBase};

use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::contract::interface_traits::{CallAs, CwOrchExecute, CwOrchQuery};
use cw_orch::environment::{BankQuerier, CwEnv, MutCwEnv, TxHandler};
use cw_orch::osmosis_test_tube::osmosis_test_tube::SigningAccount;

use cw_plus_interface::cw20_base::Cw20Base;
use speculoos::assert_that;
use speculoos::iter::ContainingIntoIterAssertions;
use speculoos::numeric::OrderedAssertions;
use speculoos::vec::VecAssertions;
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
        let config: Config = self.autocompounder_app.config()?;
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
        let config: Config = self.autocompounder_app.config()?;
        match config.vault_token {
            AssetInfoBase::Cw20(c) => {
                let vault_token = Cw20Base::new(c, self.chain.clone());
                Ok(vault_token.balance(account.into().clone())?.balance.u128())
            }
            // @Buckram123 HELP: how do i Properly handle the balance().unwrap()
            AssetInfoBase::Native(denom) => Ok(self
                .chain
                .bank_querier()
                .balance(account.into(), Some(denom))
                .unwrap()
                .first()
                .unwrap()
                .amount
                .u128()),
            _ => panic!("invalid vault token"),
        }
    }

    pub fn asset_balance<S: Into<String>>(
        &self,
        account: S,
        asset_info: &AssetInfo,
    ) -> Result<u128, Error> {
        match asset_info.clone() {
            AssetInfoBase::Cw20(c) => {
                let cw20_asset = Cw20Base::new(c, self.chain.clone());
                Ok(cw20_asset.balance(account.into())?.balance.u128())
            }
            AssetInfoBase::Native(denom) => Ok(self
                .chain
                .bank_querier()
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
        let dex_base = self.dex.dex_base();
        self.asset_balance(account, &dex_base.asset_a().asset_info)
    }

    pub fn asset_b_balance(&self, account: String) -> Result<u128, Error> {
        let dex_base = self.dex.dex_base();
        self.asset_balance(account, &dex_base.asset_b().asset_info)
    }

    pub fn pool_assets_balances<S: Into<String> + Clone>(
        &self,
        account: S,
    ) -> Result<(u128, u128), Error> {
        Ok((
            self.asset_a_balance(account.clone().into())?,
            self.asset_b_balance(account.into())?,
        ))
    }

    // CONVENIENCE DEX FUNCTIONS
    pub fn reward_token(&self) -> AssetInfo {
        let dex_base = self.dex.dex_base();
        // NOTE: Asuming a single reward token
        dex_base.reward_tokens.first().unwrap().clone()
    }

    // CONVENIENCE QUERY FUNCTIONS

    pub fn pending_claims(&self, account: &Addr) -> Result<u128, Error> {
        Ok(self
            .autocompounder_app
            .pending_claims(account.clone())?
            .into())
    }

    pub fn total_lp_position(&self) -> Result<u128, Error> {
        Ok(self.autocompounder_app.total_lp_position()?.u128())
    }

    pub fn total_supply(&self) -> Result<u128, Error> {
        Ok(self.autocompounder_app.total_supply()?.u128())
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
        let autocompounder_addr = self.autocompounder_app.addr_str()?;

        let asset_a = dex_base.asset_a();
        let asset_b = dex_base.asset_b();

        let assets = vec![(&asset_a, amount_a), (&asset_b, amount_b)]
            .into_iter()
            .filter(|(_, amount)| *amount > 0)
            .collect::<Vec<_>>();

        println!("Depositing assets: {:?}", assets);

        let ans_assets_deposit_coins = assets
            .into_iter()
            .map(|(asset, amount)| {
                self.asset_amount_to_deposit(depositor, autocompounder_addr.clone(), amount, asset)
            })
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

    fn provide_liquidity(
        &self,
        depositor: &Chain::Sender,
        amount_a: u128,
        amount_b: u128,
        recipient: Option<Addr>,
    ) -> Result<(), Error> {
        let dex_base = self.dex.dex_base();
        let dex_addr = self.dex_adapter.addr_str()?; // #FIXME: This should be the actual dex adapter address

        let asset_a = dex_base.asset_a();
        let asset_b = dex_base.asset_b();

        let assets = vec![(&asset_a, amount_a), (&asset_b, amount_b)]
            .into_iter()
            .filter(|(_, amount)| *amount > 0)
            .collect::<Vec<_>>();

        println!("Depositing assets: {:?}", assets);

        let ans_assets_deposit_coins = assets
            .into_iter()
            .map(|(asset, amount)| {
                self.asset_amount_to_deposit(depositor, dex_addr.clone(), amount, asset)
            })
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

        self.dex_adapter.execute(
            &DexExecuteMsg::AnsAction {
                dex: self.dex.name().to_string(),
                action: DexAnsAction::ProvideLiquidity {
                    assets: vec![],
                    max_spread: None,
                },
            }
            .into(),
            Some(&deposit_coins),
        )?;
        Ok(())
    }

    fn asset_amount_to_deposit<S: Into<String>>(
        &self,
        depositor: &Chain::Sender,
        spender: S,
        amount: u128,
        asset: &AssetWithInfo,
    ) -> Result<(AnsAsset, Option<Coin>), Error> {
        match &asset.asset_info {
            AssetInfoBase::Cw20(addr) => {
                let cw20_asset = Cw20Base::new(addr, self.chain.clone());
                cw20_asset.call_as(depositor).increase_allowance(
                    amount.into(),
                    spender.into(),
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

    pub fn mint_lp_tokens<S: Into<String>>(
        &self,
        minter: &Chain::Sender,
        minter_addr: S,
        recipients: Vec<S>,
        amount: u128,
    ) -> Result<Asset, anyhow::Error> {
        let dex_base = self.dex.dex_base();
        let dex_addr = self.dex_adapter.addr_str()?;

        self.provide_liquidity(minter, 1_000_000_000_000, 1_000_000_000_000, None)?;
        let lp_asset_info = self.dex.lp_asset();
        let lp_asset_amount = self.asset_balance(minter_addr.into(), &lp_asset_info)?;

        // check if the minter has enough lp tokens for all the recipients
        if lp_asset_amount > (amount * recipients.len() as u128) {
            panic!("minter does not have enough lp tokens");
        }

        let lp_asset = Asset::new(lp_asset_info.clone(), amount);
        // # TODO: Send the lp tokens to the recipients

        Ok(lp_asset)
    }

    pub fn deposit_lp_token(
        &self,
        sender: &Chain::Sender,
        amount: u128,
        recipient: Option<Addr>,
    ) -> Result<(), Error> {
        let lp_asset_entry = AnsEntryConvertor::new(self.dex.lp_token()).asset_entry();
        let lpasset_info = self.dex.lp_asset();

        let denom = match lpasset_info {
            AssetInfoBase::Native(denom) => denom,
            _ => panic!("invalid asset_info"),
        };
        self.autocompounder_app.deposit_lp(
            AnsAsset::new(lp_asset_entry, amount),
            recipient,
            &coins(amount, denom),
        )?;

        Ok(())
    }

    pub fn donate_lp_to_liquidity_pool(
        &self,
        sender: &Chain::Sender,
        amount: u128,
    ) -> Result<(), Error> {
        todo!("currently too hard to implement without the staking adapter allowing to stake lp tokens");

        let config: Config = self.autocompounder_app.config()?;
        let dex_base = self.dex.dex_base();
        let lp_token = self.dex.lp_token();
        let lp_asset_entry = AnsEntryConvertor::new(lp_token).asset_entry();

        let staking = self.staking_adapter;
    //     staking
    //         .stake(
    //             AnsAsset::new(lp_asset_entry, amount),
    //             self.dex.name().to_string(),
    //             config.unbonding_period,
    //             self.autocompounder_app, // NOTE This is wrong, we want to stake for the sender
    //         )
    //         .map_err(|e| e.into());
    }
}

impl<Chain: CwEnv, Dex: DexInit> GenericVault<Chain, Dex> {
    pub fn assert_expected_shares(
        &self,
        prev_lp_amount: u128,
        prev_total_supply: u128,
        prev_vt_balance: u128,
        account: &Addr,
    ) -> Result<u128, Error> {
        let new_lp_amount = self.autocompounder_app.total_lp_position()?.u128() - prev_lp_amount;
        assert_that!(new_lp_amount).is_greater_than(0u128);

        let gained_shares = self.vault_token_balance(account.clone())? - prev_vt_balance;
        assert_that!(gained_shares).is_greater_than(0u128);

        let expected_gained_shares: u128 = convert_to_shares(
            new_lp_amount.into(),
            prev_lp_amount.into(),
            prev_total_supply.into(),
            None,
        )
        .into();
        assert_that!(gained_shares).is_equal_to(expected_gained_shares);

        Ok(gained_shares)
    }

    pub fn assert_redeem_before_unbonding(
        &self,
        account: &Addr,
        prev_lp_amount: u128,
        prev_vt_balance: u128,
        redeem_amount: u128,
        previous_pending: u128,
        reciever: Option<&Addr>,
    ) -> Result<(), Error> {
        let vt_balance = self.vault_token_balance(account.clone())?;
        assert_that!(vt_balance).is_equal_to(prev_vt_balance - redeem_amount);

        // redeem with unbonding doesnt change the lp amount without unbonding
        let lp_amount = self.autocompounder_app.total_lp_position()?.u128();
        assert_that!(lp_amount).is_equal_to(prev_lp_amount);

        let pending_claims = if let Some(reciever) = reciever {
            let account_pending_claims = self.pending_claims(&account)?;
            assert_that!(account_pending_claims).is_equal_to(0u128);

            self.pending_claims(reciever)?
        } else {
            self.pending_claims(account)?
        };
        assert_that!(pending_claims).is_equal_to(redeem_amount + previous_pending);

        // in case of no other accounts, the vault token balance should be equal to the pending claims,
        // otherwise it should be greater than or equal pending claims for the account
        let ac_vt_balance = self.vault_token_balance(self.autocompounder_app.addr_str()?)?;
        assert_that!(ac_vt_balance).is_greater_than_or_equal_to(pending_claims);

        Ok(())
    }

    pub fn assert_batch_unbond(
        &self,
        prev_lp_amount: u128,
        redeem_amount: u128,
    ) -> Result<(), Error> {
        let prev_total_supply = self.autocompounder_app.total_supply()?.u128();
        let all_pending_claims: Vec<(Addr, Uint128)> =
            self.autocompounder_app.all_pending_claims(None, None)?;
        let all_claims: Vec<(Addr, Vec<Claim>)> = self.autocompounder_app.all_claims(None, None)?;

        // TODO: What happens if the pending claims are empty? currently, nothing i think. Just a note to check this
        //       so in both cases this batch_unbond should work
        self.autocompounder_app.batch_unbond(None, None)?;

        // after batch-unbonding, the ac vault token balance should always be 0
        assert_that!(self.vault_token_balance(self.autocompounder_app.addr_str()?)?)
            .is_equal_to(0u128);

        // after batch-unbonding, all the pending claims should be 0
        // assuming that the batch-unbonding is done for all the accounts
        let pending_claims: Vec<(Addr, Uint128)> =
            self.autocompounder_app.all_pending_claims(None, None)?;
        assert_that!(pending_claims).is_empty();

        // after batch-unbonding, the lp amount should be reduced by the redeem amount
        // #TODO: This should actually be a bit different. it should use the convert to assets function
        let lp_amount = self.autocompounder_app.total_lp_position()?.u128();
        let expected_unbonded_lp = convert_to_assets(
            redeem_amount.into(),
            prev_lp_amount.into(),
            prev_total_supply.into(),
            None,
        ).u128();

        assert_that!(prev_lp_amount - lp_amount).is_equal_to(expected_unbonded_lp);

        // the total supply of the vault token should be reduced by the redeem amount
        assert_that!(self.autocompounder_app.total_supply()?.u128())
            .is_equal_to(prev_total_supply - redeem_amount);

        // after batch unbonding all the pending claim amounts should have been added to the claims
        let all_claims_after: Vec<(Addr, Vec<Claim>)> =
            self.autocompounder_app.all_claims(None, None)?;
        if !all_pending_claims.is_empty() {
            assert_that!(all_claims_after).is_not_equal_to(all_claims.clone());
        } else {
            assert_that!(all_claims_after).is_equal_to(all_claims.clone());
        }

        all_pending_claims.iter().for_each(|(addr, claim)| {
            let claim = claim.u128();
            let previous_claims = all_claims
                .iter()
                .find(|(a, _)| a == addr)
                .unwrap()
                .clone()
                .1;
            let claims = all_claims_after
                .iter()
                .find(|(a, _)| a == addr)
                .unwrap()
                .clone()
                .1;
            assert_that!(claims.len()).is_equal_to(previous_claims.len() + 1);

            let mut claims = claims.clone();
            let new_claims = claims.split_off(previous_claims.len() - 1);
            assert_that!(claims).equals_iterator(&previous_claims.iter());

            assert_that!(new_claims.len()).is_equal_to(1);
            assert_that!(new_claims
                .first()
                .unwrap()
                .amount_of_vault_tokens_to_burn
                .u128())
            .is_equal_to(claim);
        });

        Ok(())
    }

    pub fn withdraw_and_assert(
        &self,
        user: &Chain::Sender,
        user_addr: &Addr,
        expected_balance: (u128, u128),
    ) -> AResult {
        self.autocompounder_app.call_as(user).withdraw()?;

        assert_that!(self.pool_assets_balances(user_addr)?).is_equal_to(expected_balance);
        Ok(())
    }
}

// NOTE: I think because Osmosis has only native assets, and Astroport has both, we should have 3 environments:
// - Osmosis environment with only native asset pools
// - Astroport environment with only cw20 asset pools
// - Astroport environment with both native and cw20 pools
//
// in this way, we can run the same tests for cw20 and native pools.?
