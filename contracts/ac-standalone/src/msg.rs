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
//!                 init_msg: Some(to_binary(&autocompounder_init_msg).unwrap()),
//!        };
//! // Call create_module_msg on manager
//! ```
//!
//! ## Migration
//! Migrating this contract is done by calling `ExecuteMsg::Upgrade` on [`crate::manager`] with `crate::AUTOCOMPOUNDER` as module.

use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use cw_asset::{AssetInfo, AssetInfoBase};
use cw_utils::{Duration, Expiration};

use crate::api::dex_interface::DexConfiguration;

pub const AUTOCOMPOUNDER: &str = "4t2:autocompounder";

/// Impls for being able to call methods on the autocompounder app directly
pub type QueryMsg = AutocompounderQueryMsg;
pub type InstantiateMsg = AutocompounderInstantiateMsg;
pub type MigrateMsg = AutocompounderMigrateMsg;
pub type ExecuteMsg = AutocompounderExecuteMsg;

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
    pub dex: String,
    /// Assets in the pool
    pub pool_assets: Vec<cw_asset::Asset>,
    /// Bonding period selector
    pub preferred_bonding_period: BondingPeriodSelector,
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
        funds: Vec<cw_asset::Asset>,
        recipient: Option<Addr>,
        max_spread: Option<Decimal>,
    },
    /// Deposit LP tokens. Requires approval for cw20 tokens
    DepositLp {
        lp_token: cw_asset::Asset,
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
        preferred_bonding_period: BondingPeriodSelector,
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
    pub dex_config: DexConfiguration,
    /// Address of the staking contract
    pub staking_target: StakingTarget,
    /// Pool address (number or Address)
    pub Liquidity_pool: LiquidityPool,
    /// dexname
    pub dex: String,
    /// Resolved pool assets
    pub pool_assets: Vec<AssetInfo>,
    /// pool asset names
    pub pool_asset_names: Vec<String>,
    /// Address of the LP token contract
    pub liquidity_token: AssetInfo,
    /// Vault token
    pub vault_token: AssetInfo,
    /// Pool bonding period
    pub unbonding_period: Option<Duration>,
    /// minimum unbonding cooldown
    pub min_unbonding_cooldown: Option<Duration>,
    /// maximum compound spread
    pub max_swap_spread: Decimal,
}

pub enum StakingTarget {
    Contract(Addr),
    Id(u64)
}

#[cosmwasm_schema::cw_serde]
pub struct LiquidityPoolConfig {
    staking_target: StakingTarget,
}

enum LiquidityPool {
    Contract(Addr),
    Id(u64),
}


#[cosmwasm_schema::cw_serde]
pub enum BondingPeriodSelector {
    Shortest,
    Longest,
    Custom(Duration),
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
