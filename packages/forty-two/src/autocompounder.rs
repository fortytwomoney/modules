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

use abstract_sdk::os::app;
use abstract_sdk::os::dex::{DexName, OfferAsset};
use abstract_sdk::os::objects::{AssetEntry, PoolAddress, PoolMetadata};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Decimal};
use cw_utils::Duration;

pub const AUTOCOMPOUNDER: &str = "4t2:autocompounder";

/// Impls for being able to call methods on the autocompounder app directly
pub type ExecuteMsg = app::ExecuteMsg<AutocompounderExecuteMsg>;
pub type QueryMsg = app::QueryMsg<AutocompounderQueryMsg>;
impl app::AppExecuteMsg for AutocompounderExecuteMsg {}
impl app::AppQueryMsg for AutocompounderQueryMsg {}

/// Migrate msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderMigrateMsg {}

/// Init msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderInstantiateMsg {
    pub performance_fees: Decimal,
    pub deposit_fees: Decimal,
    pub withdrawal_fees: Decimal,
    pub fee_asset: String,
    /// address that receives the fee commissions
    pub commission_addr: String,
    /// cw20 code id
    pub code_id: u64,
    /// Name of the target dex
    pub dex: DexName,
    /// Assets in the pool
    pub pool_assets: Vec<AssetEntry>,
    /// Bonding period selector
    pub preferred_bonding_period: BondingPeriodSelector,
}

#[cosmwasm_schema::cw_serde]
#[cfg_attr(feature = "boot", derive(boot_core::ExecuteFns))]
#[cfg_attr(feature = "boot", impl_into(ExecuteMsg))]
pub enum AutocompounderExecuteMsg {
    UpdateFeeConfig {
        performance: Option<Decimal>,
        deposit: Option<Decimal>,
        withdrawal: Option<Decimal>,
    },
    /// Join vault by depositing one or more funds
    Deposit { funds: Vec<OfferAsset> },
    /// Withdraw all unbonded funds
    Withdraw {},
    /// Compound all rewards in the vault
    Compound {},
    /// Unbond in batches
    BatchUnbond {},
}

#[cosmwasm_schema::cw_serde]
#[derive(QueryResponses)]
#[cfg_attr(feature = "boot", derive(boot_core::QueryFns))]
#[cfg_attr(feature = "boot", impl_into(QueryMsg))]
pub enum AutocompounderQueryMsg {
    /// Query the config of the autocompounder
    /// Returns [`Config`]
    #[returns(Config)]
    Config {},
}

#[cosmwasm_schema::cw_serde]
pub enum Cw20HookMsg {
    /// Withdraws a given amount from the vault.
    Redeem {},
}

#[cosmwasm_schema::cw_serde]
pub struct FeeConfig {
    pub performance: Decimal,
    pub deposit: Decimal,
    pub withdrawal: Decimal,
    pub fee_asset: AssetEntry,
}

#[cosmwasm_schema::cw_serde]
pub struct Config {
    /// Address of the staking contract
    pub staking_contract: Addr,
    /// Pool address (number or Address)
    pub pool_address: PoolAddress,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Address of the LP token contract
    pub liquidity_token: Addr,
    /// Vault token
    pub vault_token: Addr,
    /// Address that receives the fee commissions
    pub commission_addr: Addr,
    /// Vault fee structure
    pub fees: FeeConfig,
    /// Pool bonding period
    pub unbonding_period: Option<Duration>,
    /// minimum unbonding cooldown
    pub min_unbonding_cooldown: Option<Duration>,
}

#[cosmwasm_schema::cw_serde]
pub enum BondingPeriodSelector {
    Shortest,
    Longest,
    Custom(Duration),
}
