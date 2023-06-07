//! # App InterchainSwapRouter
//!
//! `your_namespace::interchain-swap-router` is an app which allows users to ...
//!
//! ## Creation
//! The contract can be added on an OS by calling [`ExecuteMsg::CreateModule`](crate::manager::ExecuteMsg::CreateModule) on the manager of the os.
//! ```ignore
//! let interchain-swap-router_init_msg = InstantiateMsg::InterchainSwapRouterInstantiateMsg{
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
//!                 init_msg: Some(to_binary(&interchain-swap-router_init_msg).unwrap()),
//!        };
//! // Call create_module_msg on manager
//! ```
//!
//! ## Migration
//! Migrating this contract is done by calling `ExecuteMsg::Upgrade` on [`crate::manager`] with `crate::AUTOCOMPOUNDER` as module.

use abstract_dex_adapter::msg::OfferAsset;
use abstract_sdk::os::app;
use abstract_sdk::os::objects::{AssetEntry, DexName, PoolAddress, PoolMetadata};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_asset::AssetInfo;
use cw_utils::{Duration, Expiration};

pub const SWAPROUTER: &str = "4t2:swaprouter";

/// Impls for being able to call methods on the interchain-swap-router app directly
pub type ExecuteMsg = app::ExecuteMsg<InterchainSwapRouterExecuteMsg, Cw20ReceiveMsg>;
pub type QueryMsg = app::QueryMsg<InterchainSwapRouterQueryMsg>;
pub type InstantiateMsg = app::InstantiateMsg<InterchainSwapRouterInstantiateMsg>;
pub type MigrateMsg = app::MigrateMsg<InterchainSwapRouterMigrateMsg>;

impl app::AppExecuteMsg for InterchainSwapRouterExecuteMsg {}
impl app::AppQueryMsg for InterchainSwapRouterQueryMsg {}

/// Migrate msg
#[cosmwasm_schema::cw_serde]
pub struct InterchainSwapRouterMigrateMsg {}

/// Init msg
#[cosmwasm_schema::cw_serde]
pub struct InterchainSwapRouterInstantiateMsg {}

#[cosmwasm_schema::cw_serde]
#[cfg_attr(feature = "boot", derive(boot_core::ExecuteFns))]
#[cfg_attr(feature = "boot", impl_into(ExecuteMsg))]
pub enum InterchainSwapRouterExecuteMsg {
    /// Join vault by depositing one or more funds
    #[payable]
    Deposit {
        funds: Vec<OfferAsset>,
    },
    Swap {
        swap: Swap,
    },
}

#[cosmwasm_schema::cw_serde]
#[derive(QueryResponses)]
#[cfg_attr(feature = "boot", derive(boot_core::QueryFns))]
#[cfg_attr(feature = "boot", impl_into(QueryMsg))]
pub enum InterchainSwapRouterQueryMsg {
    /// Query the config of the interchain-swap-router
    /// Returns [`Config`]
    #[returns(Config)]
    Config {},
    /// Query the balance of vault tokens of a given address
    /// Returns [`Uint128`]
    #[returns(Uint128)]
    Balance { address: String },
}

#[cosmwasm_schema::cw_serde]
pub enum Cw20HookMsg {
    /// Withdraws a given amount from the vault.
    Swap {},
}

/// Vault fee structure
#[cosmwasm_schema::cw_serde]
pub struct FeeConfig {
    pub performance: Decimal,
    pub deposit: Decimal,
    pub withdrawal: Decimal,
    pub fee_asset: AssetEntry,
    /// Address that receives the fee commissions
    pub commission_addr: Addr,
}

#[cosmwasm_schema::cw_serde]
pub struct Config {
    pub manager: Addr,
}

#[cosmwasm_schema::cw_serde]
pub struct Route {
    pub from: AssetEntry,
    pub to: AssetEntry,
    pub dex: DexName,
    pub chain: Chain,
    pub slippage: Option<Decimal>,
}
// The swap router should be able to find the router per chain by querying ans

#[cosmwasm_schema::cw_serde]
pub struct Swap {
    pub id: Option<u64>,
    pub initial_asset: AssetEntry, // with amount
    pub origin_addr: Addr,
    pub origin_chain: Chain,
    pub final_addr: Addr,
    pub final_chain: Chain,
    pub slippage: Option<Decimal>,
    pub routes: Vec<Route>,
    pub prev_routes: Vec<Route>,
}
