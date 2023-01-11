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
use abstract_sdk::os::objects::AssetEntry;
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::Decimal;

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
    // pub staking_contract: String,
    // pub liquidity_token: String,
    pub performance_fees: Decimal,
    pub deposit_fees: Decimal,
    pub withdrawal_fees: Decimal,
    pub fee_asset: String,
    /// address that recieves the fee commissions
    pub commission_addr: String,
    /// cw20 code id
    pub code_id: u64,
    /// Name of the target dex
    pub dex: DexName,
    /// Assets in the pool
    pub pool_assets: Vec<AssetEntry>,
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
    /// Returns [`ConfigResponse`]
    #[returns(ConfigResponse)]
    Config {},
}

#[cosmwasm_schema::cw_serde]
pub enum Cw20HookMsg {
    /// Withdraws a given amount from the vault.
    Redeem {},
}

#[cosmwasm_schema::cw_serde]
pub struct ConfigResponse {}
