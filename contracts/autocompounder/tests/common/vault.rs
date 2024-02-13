use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_interface::{Abstract, AbstractAccount};
use anyhow::Error;
use autocompounder::interface::AutocompounderApp;
use autocompounder::msg::AutocompounderExecuteMsgFns;
use autocompounder::msg::AutocompounderQueryMsgFns;
use cosmwasm_std::{coins, Addr};
use cw20::msg::Cw20ExecuteMsgFns;
use cw_asset::{Asset, AssetInfo, AssetInfoBase};
use cw_orch::contract::interface_traits::CallAs;
use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::environment::{CwEnv, MutCwEnv};
use cw_plus_interface::cw20_base::Cw20Base;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20Base<Chain>,
    pub staking: CwStakingAdapter<Chain>,
    pub dex: DexAdapter<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}

pub struct GenericVault<Chain: MutCwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: AssetInfoBase<Addr>,
    pub staking: CwStakingAdapter<Chain>,
    pub dex: DexAdapter<Chain>,
    pub abstract_core: Abstract<Chain>,
    pub chain: Chain,
}

impl<T: MutCwEnv> GenericVault<T> {
    pub fn redeem_vault_token(
        self,
        amount: u128,
        sender: &Addr,
        reciever: Option<Addr>,
    ) -> Result<<T as cw_orch::prelude::TxHandler>::Response, Error> {
        let config = self.auto_compounder.config()?;
        match config.vault_token {
            AssetInfoBase::Cw20(c) => {
                let vault_token = Cw20Base::new(c, self.chain);
                let res = vault_token.call_as(sender).increase_allowance(
                    amount.into(),
                    self.auto_compounder.addr_str()?,
                    None,
                )?;

                Ok(self
                    .auto_compounder
                    .call_as(sender)
                    .redeem(amount.into(), reciever, &[])?)
            }
            AssetInfoBase::Native(denom) => {
                let res = self.auto_compounder.call_as(sender).redeem(
                    amount.into(),
                    reciever,
                    coins(amount, denom),
                );
                Ok(res)
            }
            _ => panic!("invalid vault token"),
        }
    }
}
