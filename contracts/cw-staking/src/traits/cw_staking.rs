use abstract_sdk::feature_objects::AnsHost;
use abstract_sdk::os::objects::{AssetEntry, ContractEntry};
use cosmwasm_std::{Addr, CosmosMsg, Deps, QuerierWrapper, StdResult, Uint128};
use cw_asset::Asset;
use forty_two::cw_staking::{Claim, StakingInfoResponse};

use crate::error::StakingError;
use crate::traits::identify::Identify;

/// Trait that defines the interface for staking providers
pub trait CwStaking: Identify {
    // TODO: Move to SDK.
    /// Construct a staking contract entry from the staking token and the provider
    fn staking_entry(&self, staking_token: &AssetEntry) -> ContractEntry {
        ContractEntry {
            protocol: self.name().to_string(),
            contract: format!("staking/{staking_token}"),
        }
    }
    // TODO: Move to SDK.
    /// Retrieve the staking contract address for the pool with the provided staking token name
    fn staking_contract_address(
        &self,
        deps: Deps,
        ans_host: &AnsHost,
        staking_token: &AssetEntry,
    ) -> StdResult<Addr> {
        let provider_staking_contract_entry = self.staking_entry(staking_token);
        ans_host.query_contract(&deps.querier, &provider_staking_contract_entry)
    }

    /// Stake the provided asset into the staking contract
    ///
    /// * `deps` - the dependencies
    /// * `staking_address` - the address of the staking contract
    /// * `asset` - the asset to stake
    fn stake(
        &self,
        deps: Deps,
        staking_address: Addr,
        asset: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError>;

    /// Stake the provided asset into the staking contract
    ///
    /// * `deps` - the dependencies
    /// * `staking_address` - the address of the staking contract
    /// * `asset` - the asset to stake
    fn unstake(
        &self,
        deps: Deps,
        staking_address: Addr,
        amount: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError>;

    /// Claim rewards on the staking contract
    ///
    /// * `deps` - the dependencies
    /// * `staking_address` - the address of the staking contract
    fn claim(&self, deps: Deps, staking_address: Addr) -> Result<Vec<CosmosMsg>, StakingError>;

    fn query_info(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
    ) -> StdResult<StakingInfoResponse>;
    // This function queries the staked token balance of a staker
    // The staking contract is queried using the staking address
    fn query_staked(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
        staker: Addr,
    ) -> StdResult<Uint128>;
    fn query_unbonding(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
        staker: Addr,
    ) -> StdResult<Vec<Claim>>;
}
