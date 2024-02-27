//! # App Autocompounder
//!
//! `your_namespace::autocompounder` is an app which allows users to ...
//!
//! ## Creation
//! The contract can be added on an Account by calling [`ExecuteMsg::CreateModule`](crate::manager::ExecuteMsg::CreateModule) on the manager of the account.
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
//!                 init_msg: Some(to_json_binary(&autocompounder_init_msg).unwrap()),
//!        };
//! // Call create_module_msg on manager
//! ```
//!
//! ## Migration
//! Migrating this contract is done by calling `ExecuteMsg::Upgrade` on [`crate::manager`] with `crate::AUTOCOMPOUNDER` as module.

use abstract_app::objects::AnsAsset;
use abstract_core::objects::{AnsEntryConvertor, LpToken};
use abstract_sdk::core::app;
use abstract_sdk::core::objects::{AssetEntry, DexName, PoolAddress, PoolMetadata};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_asset::{AssetInfo, AssetInfoBase};
use cw_utils::{Duration, Expiration};

pub const AUTOCOMPOUNDER: &str = "autocompounder";
pub const AUTOCOMPOUNDER_ID: &str = "4t2:autocompounder";

/// Impls for being able to call methods on the autocompounder app directly
pub type ExecuteMsg = app::ExecuteMsg<AutocompounderExecuteMsg, Cw20ReceiveMsg>;
pub type QueryMsg = app::QueryMsg<AutocompounderQueryMsg>;
pub type InstantiateMsg = app::InstantiateMsg<AutocompounderInstantiateMsg>;
pub type MigrateMsg = app::MigrateMsg<AutocompounderMigrateMsg>;

impl app::AppExecuteMsg for AutocompounderExecuteMsg {}
impl app::AppQueryMsg for AutocompounderQueryMsg {}

/// Migrate msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderMigrateMsg {
    pub version: String,
}

/// Init msg
#[cosmwasm_schema::cw_serde]
pub struct AutocompounderInstantiateMsg {
    pub performance_fees: Decimal,
    pub deposit_fees: Decimal,
    pub withdrawal_fees: Decimal,
    /// address that receives the fee commissions
    pub commission_addr: String,
    /// cw20 code id
    pub code_id: Option<u64>,
    /// Name of the target dex
    pub dex: DexName,
    /// Assets in the pool
    pub pool_assets: Vec<AssetEntry>,
    /// Unbonding data for manual setup
    pub bonding_data: Option<BondingData>,
    /// max swap spread
    pub max_swap_spread: Option<Decimal>,
}

#[cosmwasm_schema::cw_serde]
#[cfg_attr(feature = "interface", derive(cw_orch::ExecuteFns))]
#[cfg_attr(feature = "interface", impl_into(ExecuteMsg))]
pub enum AutocompounderExecuteMsg {
    UpdateFeeConfig {
        performance: Option<Decimal>,
        deposit: Option<Decimal>,
        withdrawal: Option<Decimal>,
        fee_collector_addr: Option<String>,
    },
    /// Join vault by depositing one or more funds. Requires approval for cw20 tokens
    #[cfg_attr(feature = "interface", payable)]
    Deposit {
        funds: Vec<AnsAsset>,
        recipient: Option<Addr>,
        max_spread: Option<Decimal>,
    },
    /// Deposit LP tokens. Requires approval for cw20 tokens
    DepositLp {
        lp_token: AnsAsset,
        recipient: Option<Addr>,
    },
    Redeem {
        amount: Uint128,
        recipient: Option<Addr>,
    },
    /// Withdraw all unbonded funds
    Withdraw {},
    /// Compound all rewards in the vault
    Compound {},
    /// Unbond in batches
    BatchUnbond {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    // Updates min_unbonding_cooldown and unbonding_period in the config with the latest staking contract data
    UpdateStakingConfig {
        bonding_data: Option<BondingData>,
    },
}

#[cosmwasm_schema::cw_serde]
#[derive(QueryResponses)]
#[cfg_attr(feature = "interface", derive(cw_orch::QueryFns))]
#[cfg_attr(feature = "interface", impl_into(QueryMsg))]
pub enum AutocompounderQueryMsg {
    /// Query the config of the autocompounder
    /// Returns [`Config`]
    #[returns(Config)]
    Config {},
    /// Query the fee config of the autocompounder
    /// Returns [`FeeConfig`]
    #[returns(FeeConfig)]
    FeeConfig {},
    /// Query the amount of pending claims
    /// Returns [`Uint128`]
    #[returns(Uint128)]
    PendingClaims { address: Addr },
    /// Query all pending claims
    /// Returns [`Vec<Claim>`]
    #[returns(Vec<(Addr, Uint128)>)]
    AllPendingClaims {
        start_after: Option<Addr>,
        limit: Option<u8>,
    },
    /// Query the amount of claims
    /// Returns [`Vec<Claim>`]
    #[returns(Vec<Claim>)]
    Claims { address: Addr },
    /// Query all claim accounts
    /// Returns [`Vec<(Sting, Vec<Claim>)>`]
    #[returns(Vec<(Addr, Vec<Claim>)>)]
    AllClaims {
        start_after: Option<Addr>,
        limit: Option<u8>,
    },
    /// Query the latest unbonding
    /// Returns [`Expiration`]
    #[returns(Expiration)]
    LatestUnbonding {},
    /// Query the vaults total lp position
    /// Returns [`Uint128`]
    #[returns(Uint128)]
    TotalLpPosition {},
    /// Query the vault token supply
    /// Returns [`Uint128`]
    #[returns(Uint128)]
    TotalSupply {},
    /// Query the number of assets per share(s) in the vault
    /// Returns ['Uint128']
    #[returns(Uint128)]
    AssetsPerShares { shares: Option<Uint128> },
    /// Query the balance of vault tokens of a given address
    /// Returns [`Uint128`]
    #[returns(Uint128)]
    Balance { address: Addr },
}

// #[cosmwasm_schema::cw_serde]
// pub enum Cw20HookMsg {
//     /// Withdraws a given amount from the vault.
//     // Redeem {},
// }

/// Vault fee structure
#[cosmwasm_schema::cw_serde]
pub struct FeeConfig {
    pub performance: Decimal,
    pub deposit: Decimal,
    pub withdrawal: Decimal,
    /// Address that receives the fee commissions
    pub fee_collector_addr: Addr,
}

#[cosmwasm_schema::cw_serde]
pub struct Config {
    /// Pool address (number or Address)
    pub pool_address: PoolAddress,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Resolved pool assets
    pub pool_assets: Vec<AssetInfo>,
    /// Address of the LP token contract
    pub liquidity_token: AssetInfoBase<Addr>,
    /// Vault token
    pub vault_token: AssetInfoBase<Addr>,
    /// Pool bonding period
    pub unbonding_period: Option<Duration>,
    /// minimum unbonding cooldown
    pub min_unbonding_cooldown: Option<Duration>,
    /// maximum compound spread
    pub max_swap_spread: Decimal,
}

impl Config {
    pub fn lp_token(&self) -> LpToken {
        LpToken {
            dex: self.pool_data.dex.clone(),
            assets: self.pool_data.assets.clone(),
        }
    }

    pub fn lp_asset_entry(&self) -> AssetEntry {
        AnsEntryConvertor::new(self.lp_token()).asset_entry()
    }
}

#[cosmwasm_schema::cw_serde]
pub enum BondingPeriodSelector {
    Shortest,
    Longest,
    Custom(Duration),
}

#[cosmwasm_schema::cw_serde]
pub struct BondingData {
    pub unbonding_period: Duration,
    pub max_claims_per_address: Option<u32>,
}

#[cosmwasm_schema::cw_serde]
pub struct Claim {
    // timestamp of the start of the unbonding process
    pub unbonding_timestamp: Expiration,
    // amount of vault tokens to be burned
    pub amount_of_vault_tokens_to_burn: Uint128,
    //  amount of lp tokens being unbonded
    pub amount_of_lp_tokens_to_unbond: Uint128,
}
