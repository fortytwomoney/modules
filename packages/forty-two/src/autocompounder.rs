//! # App Autocompounder
//!
//! `your_namespace::autocompounder` is an app which allows users to ...
//!
//! ## Creation
//! The contract can be added on an OS by calling [`ExecuteMsg::CreateModule`](crate::manager::ExecuteMsg::CreateModule) on the manager of the os.
//! ```ignore
//! let autocompounder_init_msg = InstantiateMsg::AutocompounderInstantiateMsg{
//!               /// The initial value for max_count
//!               pub max_count: Uint128,
//!               /// Initial user counts
//!               pub initial_counts: Option<Vec<(String, Uint128)>>,
//!           };
//!
//! let create_module_msg = ExecuteMsg::CreateModule {
//!                 module: Module {
//!                     info: ModuleInfo {
//!                         name: AUTOCOMPOUNDER.into(),
//!                         version: None,
//!                     },
//!                     kind: crate::core::modules::ModuleKind::External,
//!                 },
//!                 init_msg: Some(to_binary(&autocompounder_init_msg).unwrap()),
//!        };
//! // Call create_module_msg on manager
//! ```
//!
//! ## Migration
//! Migrating this contract is done by calling `ExecuteMsg::Upgrade` on [`crate::manager`] with `crate::AUTOCOMPOUNDER` as module.

use abstract_sdk::os::{app, dex::OfferAsset};
use cosmwasm_std::{Binary, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_asset::Asset;

pub const AUTOCOMPOUNDER: &str = "4t2:autocompounder";

/// Impls for being able to call methods on the autocompounder app directly
impl app::AppExecuteMsg for AutocompounderExecuteMsg {}
impl app::AppQueryMsg for AutocompounderQueryMsg {}

/// Migrate msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderMigrateMsg {}

/// Init msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderInstantiateMsg {
    pub staking_contract: String,
    pub liquidity_token: String,
    pub performance_fees: Uint128,
    pub deposit_fees: Uint128,
    pub withdrawal_fees: Uint128,    
    /// address that recieves the fee commissions
    pub commission_addr: String,
    /// cw20 code id
    pub code_id: u64,
}

#[cosmwasm_schema::cw_serde]
pub enum AutocompounderExecuteMsg {
    UpdateFeeConfig {
        performance: Option<Uint128>,
        deposit: Option<Uint128>,
        withdrawal: Option<Uint128>,
    },
    /// Zap in by depositing a single asset
    Zap {
        funds: Asset,
    },
    /// Join vault by depositing 2 funds
    Deposit {
        funds: Vec<Asset>,
    },
    /// Withdraw all unbonded funds
    Withdraw { },
    /// Unbond LP tokens 
    Unbond { amount: Uint128 },
    /// Compound all rewards in the vault
    Compound {},
    Receive(Cw20ReceiveMsg),
}

#[cosmwasm_schema::cw_serde]
pub enum AutocompounderQueryMsg {
    Config {},
}

#[cosmwasm_schema::cw_serde]
pub enum Cw20HookMsg {
    /// Withdraws a given amount from the vault.
    Redeem {},
}
