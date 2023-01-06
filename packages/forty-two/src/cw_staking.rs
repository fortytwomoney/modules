//! # Staking Api
//!
//! `4t2::cw-staking`

use abstract_sdk::os::api;
use abstract_sdk::os::objects::{AnsAsset, AssetEntry};
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Addr, Uint128};
use cw20::Expiration;
use cw_asset::AssetInfo;
use cw_utils::Duration;

pub type ProviderName = String;

/// The callback id for staking over ibc
pub const IBC_STAKING_PROVIDER_ID: u32 = 22335;

pub const CW_STAKING: &str = "4t2:cw_staking";

pub type ExecuteMsg = api::ExecuteMsg<CwStakingExecuteMsg>;
pub type QueryMsg = api::QueryMsg<CwStakingQueryMsg>;

impl api::ApiExecuteMsg for CwStakingExecuteMsg {}

impl api::ApiQueryMsg for CwStakingQueryMsg {}

/// A request message that's sent to this staking api
#[cosmwasm_schema::cw_serde]
pub struct CwStakingExecuteMsg {
    /// The name of the staking provider
    pub provider: ProviderName,
    pub action: CwStakingAction,
}

#[cosmwasm_schema::cw_serde]
/// Possible actions to perform on the staking contract
pub enum CwStakingAction {
    /// Stakes/bonds a given token
    Stake { staking_token: AnsAsset },
    /// Unstake a given token
    Unstake { staking_token: AnsAsset },
    /// Claim rewards for a given token
    ClaimRewards { staking_token: AssetEntry },
}

#[cosmwasm_schema::cw_serde]
#[derive(QueryResponses)]
pub enum CwStakingQueryMsg {
    #[returns(StakingInfoResponse)]
    Info {
        provider: ProviderName,
        staking_token: AssetEntry,
    },
    #[returns(StakeResponse)]
    Staked {
        provider: ProviderName,
        staking_token: AssetEntry,
        staker_address: String,
    },
    #[returns(UnbondingResponse)]
    Unbonding {
        provider: ProviderName,
        staking_token: AssetEntry,
        staker_address: String,
    },
}

#[cosmwasm_schema::cw_serde]
pub struct StakingInfoResponse {
    pub unbonding_period: Option<Duration>,
    pub staking_contract_address: Addr,
    pub staking_token: AssetInfo,
    pub max_claims: Option<u32>,
}

#[cosmwasm_schema::cw_serde]
pub struct StakeResponse {
    pub amount: Uint128,
}

#[cosmwasm_schema::cw_serde]
pub struct UnbondingResponse {
    pub claims: Vec<Claim>,
}

#[cosmwasm_schema::cw_serde]
pub struct Claim {
    pub amount: Uint128,
    pub claimable_at: Expiration,
}
