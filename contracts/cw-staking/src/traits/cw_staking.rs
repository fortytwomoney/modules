use abstract_sdk::feature_objects::AnsHost;
use abstract_sdk::os::objects::{AssetEntry, ContractEntry};
use cosmwasm_std::{Addr, CosmosMsg, Deps, QuerierWrapper, StdResult, Uint128};

use forty_two::cw_staking::{StakeResponse, StakingInfoResponse, UnbondingResponse};

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

    /// Fetch the required data for interacting with the provider
    fn fetch_data(
        &mut self,
        deps: Deps,
        ans_host: &AnsHost,
        staking_asset: AssetEntry,
    ) -> StdResult<()>;

    /// Stake the provided asset into the staking contract
    ///
    /// * `deps` - the dependencies
    /// * `asset` - the asset to stake
    fn stake(&self, deps: Deps, amount: Uint128) -> Result<Vec<CosmosMsg>, StakingError>;

    /// Stake the provided asset into the staking contract
    ///
    /// * `deps` - the dependencies
    /// * `asset` - the asset to stake
    fn unstake(&self, deps: Deps, amount: Uint128) -> Result<Vec<CosmosMsg>, StakingError>;

    /// Claim rewards on the staking contract
    ///
    /// * `deps` - the dependencies
    fn claim(&self, deps: Deps) -> Result<Vec<CosmosMsg>, StakingError>;

    fn query_info(&self, querier: &QuerierWrapper) -> StdResult<StakingInfoResponse>;
    // This function queries the staked token balance of a staker
    // The staking contract is queried using the staking address
    fn query_staked(&self, querier: &QuerierWrapper, staker: Addr) -> StdResult<StakeResponse>;
    fn query_unbonding(
        &self,
        querier: &QuerierWrapper,
        staker: Addr,
    ) -> StdResult<UnbondingResponse>;
}
