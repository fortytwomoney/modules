use astroport::pair::SimulationResponse;
/// This file should start of simple and have only one integration done. Dont start with all at once.
use cosmwasm_std::{Addr, CosmosMsg, Decimal, Uint128, QuerierWrapper, Env};
use cw_asset::{AssetList, AssetInfo, Asset};

use super::{dex_error::DexError, dexes::astroport::{AstroportAMM, AstroportConfiguration}};
pub type DexQueryResult<T> = Result<T, DexError>;
pub type DexResult = Result<Vec<CosmosMsg>, DexError>;

/// A dex platform should be able to perform the following actions:
/// Swap
/// Provide Liquidity
/// Stake
/// Claim Rewards
/// Unstake
/// Redeem(initiate withdraw)
///
/// Each dex has its own calls and has its own data types for where to stake, swap, provide liquidity and claim rewards.
/// The goal here is to create a simple unified interface for all dexes, that is easy to use and understand, but above all,
/// Allows us to have a single repo of the contracts and not have to maintain multiple repos for each dex.
///
/// We have 3 Dexes we want to integrate. Osmosis, Astroport and Kujira.
/// For each dex there are some specific details.
/// Osmosis:
/// - All interactions are native sdk messages
/// - Pool address is an id in the form of a u64
/// - LP token is a native token
/// - lp assets can be both cw20 or native
///
/// Astroport:
/// - All interactions are Cosmwasm smartcontract messages
/// - Pool address is a bech32 address
/// - LP token is a cw20 token
/// - lp assets can be both cw20 or native
/// - requires a staking contract address and a generator address
///
/// Kujira:
/// - All interactions are Cosmwasm smartcontract messages
/// - there is a swap pool address
/// - there is a lp token denom
/// - all denoms are native sdk.Coin types
/// - requires a staking contract address

#[cosmwasm_schema::cw_serde]
pub enum DexConfiguration {
    Osmosis(OsmosisConfiguration),
    Astroport(AstroportConfiguration),
    Kujira(KujiraConfiguration),
}


pub fn create_dex_from_config<'a>(config: DexConfiguration, env: &'a Env, querier: &'a QuerierWrapper) -> BoxedDex<'a> {
    match config {
        DexConfiguration::Astroport(astroport_config) => Box::new(AstroportAMM::new( env, querier, astroport_config)),
        DexConfiguration::Osmosis(osmosis_config) => panic!("Osmosis not supported yet"),
        DexConfiguration::Kujira(kujira_config) => panic!("Kujira not supported yet"), 
    }
}
impl <'a>DexConfiguration {
    pub fn dex(&'a self, env: &'a Env, querier: &'a QuerierWrapper) -> BoxedDex<'a> {
        create_dex_from_config(self.clone(), env,querier)
    }
}


#[cosmwasm_schema::cw_serde]
pub struct OsmosisConfiguration {
    OsmosisPoolId: u64,
    // ... Osmosis-specific fields
}



#[cosmwasm_schema::cw_serde]
pub struct KujiraConfiguration {
    staking_contract_address: String,
}



/// The generic interface that all DEXes should implement.
pub trait DexInterface {
    /// Executes a kswap operation.
    ///
    /// Parameters might include the source token, target token, amount, etc.
    /// Returns a result indicating success or providing error details.
    fn swap(
        &self,
        source_token: cw_asset::Asset,
        target_token: cw_asset::AssetInfo,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
    ) -> DexResult;

    /// Provides liquidity to the DEX.
    ///
    /// Parameters might include the token pair, amount, etc.
    /// Returns a result indicating success or providing error details.
    fn provide_liquidity(
        &self,
        assets: Vec<Asset>,
        max_spread: Option<Decimal>,
    ) -> DexResult;


    /// Withdraws liquidity from the DEX.
    /// Parameters might include the token pair, amount, etc.
    /// Returns a result indicating success or providing error details.
    fn withdraw_liquidity(
        &self,
        amount: Uint128,
    ) -> DexResult;

    /// Stakes tokens in the DEX.
    ///
    /// Parameters might include the token type and amount.
    /// Returns a result indicating success or providing error details.
fn stake(&self, amount: Uint128) -> DexResult;

    /// Claims rewards from the DEX.
    ///
    /// Returns a result indicating success or providing error details.
    fn claim_rewards(&self) -> DexResult;

    /// Unstakes tokens from the DEX.
    ///
    /// Parameters might include the token type and amount.
    /// Returns a result indicating success or providing error details.
    fn unstake(&self, amount: Uint128) -> DexResult;

    /// Initiates a withdrawal (redeem) from the DEX.
    ///
    /// Parameters might include the token type and amount.
    /// Returns a result indicating success or providing error details.
    fn claim(&self) -> DexResult;

    // fn query_info(querier: &QuerierWrapper) -> DexQueryResult<()>;
    fn simulate_swap(
        &self,
        source_token: cw_asset::Asset,
        target_token: cw_asset::AssetInfo,
        belief_price: Option<Decimal>,
    ) -> DexQueryResult<SimulationResponse>;

    /// queries the staked amount of a given address
    fn query_staked(&self) -> DexQueryResult<Uint128>;

    /// queries the current unbonding of the current asset
    fn query_unbonding(&self) -> DexQueryResult<Uint128>;

    /// queries the current rewards of the current asset
    fn query_rewards(&self) -> DexQueryResult<Vec<cw_asset::AssetInfo>>;

    /// query the current balance of the lp token
    fn query_lp_balance(&self) -> DexQueryResult<Uint128>;
    /// query the current balances of the pool assets
    fn query_pool_balances(&self, owner: Addr) -> DexQueryResult<Vec<cw_asset::Asset>>;

}

pub type BoxedDex<'a> = Box<dyn DexInterface + 'a>;



// impl DexInterface for AnyDex {
//     fn from_configuration(configuration: DexConfiguration) -> Box<dyn DexInterface> {
//         // ... implementation
//     }
//     // ... other methods
// }

#[cfg(test)]
pub mod testing {
    use super::*;

    pub use crate::api::dexes::astroport::tests::create_astro_setup;

}