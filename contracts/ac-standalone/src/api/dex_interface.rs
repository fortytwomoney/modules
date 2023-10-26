/// This file should start of simple and have only one integration done. Dont start with all at once.
use cosmwasm_std::{Addr, CosmosMsg, Decimal, Uint128, QuerierWrapper};

use super::{dex_error::DexError, dexes::astroport::AstroportAMM};
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


pub fn create_dex_from_config(config: DexConfiguration) -> BoxedDex {
    match config {
        DexConfiguration::Astroport(astroport_config) => Box::new(AstroportAMM::from(astroport_config)),
        DexConfiguration::Osmosis(osmosis_config) => panic!("Osmosis not supported yet"),
        DexConfiguration::Kujira(kujira_config) => panic!("Kujira not supported yet"), 
    }
}
impl DexConfiguration {
    pub fn dex(&self) -> BoxedDex {
        create_dex_from_config(self.clone())
    }
}

impl From<DexConfiguration> for BoxedDex {
    fn from(config: DexConfiguration) -> Self {
        create_dex_from_config(config)
    }
}

#[cosmwasm_schema::cw_serde]
struct OsmosisConfiguration {
    OsmosisPoolId: u64,
    // ... Osmosis-specific fields
}

#[cosmwasm_schema::cw_serde]
struct AstroportConfiguration {
    lp_token_address: String,
    staking_contract_address: String,
    pair_address: String,
    generator_address: String,
    asset_info_a: cw_asset::AssetInfo,
    asset_info_b: cw_asset::AssetInfo,
}

#[cosmwasm_schema::cw_serde]
struct KujiraConfiguration {
    staking_contract_address: String,
}


/// The generic interface that all DEXes should implement.
pub trait DexInterface {
    /// Executes a swap operation.
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
        token_a: cw_asset::Asset,
        token_b: cw_asset::Asset,
        belief_price: Option<Decimal>,
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
    fn stake(&self, token: cw_asset::Asset, bonding_period: Option<u64>) -> DexResult;

    /// Claims rewards from the DEX.
    ///
    /// Returns a result indicating success or providing error details.
    fn claim_rewards(&self) -> DexResult;

    /// Unstakes tokens from the DEX.
    ///
    /// Parameters might include the token type and amount.
    /// Returns a result indicating success or providing error details.
    fn unstake(&self, token: cw_asset::Asset) -> DexResult;

    /// Initiates a withdrawal (redeem) from the DEX.
    ///
    /// Parameters might include the token type and amount.
    /// Returns a result indicating success or providing error details.
    fn claim(&self, token: cw_asset::AssetInfo,) -> DexResult;

    fn query_info(&self, querier: &QuerierWrapper) -> DexQueryResult<()>;

    /// queries the staked amount of a given address
    fn query_staked(&self, querier: &QuerierWrapper, staker: Addr) -> DexQueryResult<Uint128>;

    /// queries the current unbonding of the current asset
    fn query_unbonding(&self, querier: &QuerierWrapper, staker: Addr) -> DexQueryResult<Uint128>;

    /// queries the current rewards of the current asset
    fn query_rewards(&self, querier: &QuerierWrapper,) -> DexQueryResult<cw_asset::Asset>;

}

pub type BoxedDex = Box<dyn DexInterface>;
// impl DexInterface for AnyDex {
//     fn from_configuration(configuration: DexConfiguration) -> Box<dyn DexInterface> {
//         // ... implementation
//     }
//     // ... other methods
// }
