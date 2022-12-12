use cosmwasm_std::{Addr};


#[cfg(feature = "osmosis")]
use osmosis_std::types::osmosis::gamm::v1beta1::{
    MsgExitPool, MsgJoinPool, MsgSwapExactAmountIn, QuerySwapExactAmountInRequest,
    SwapAmountInRoute,
};
#[cfg(feature = "osmosis")]
use osmosis_std::types::{
    cosmos::base::v1beta1::Coin as OsmoCoin,
    osmosis::gamm::v1beta1::{Pool, QueryPoolRequest},
};


use crate::traits::identify::Identify;

pub const OSMOSIS: &str = "osmosis";
pub struct Osmosis {
    pub local_proxy_addr: Option<Addr>,
}

impl Identify for Osmosis {
    fn over_ibc(&self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        OSMOSIS
    }
}

/// Osmosis app-chain dex implementation
#[cfg(feature = "osmosis")]
impl CwStakingProvider for Osmosis {
    fn stake(&self, deps: Deps, staking_address: Addr, asset: Asset) -> Result<Vec<CosmosMsg>, StakingError> {
        unimplemented!()
    }

    fn unstake(&self, deps: Deps, staking_address: Addr, amount: Asset) -> Result<Vec<CosmosMsg>, StakingError> {
        unimplemented!()
    }

    fn claim(&self, deps: Deps, staking_address: Addr) -> Result<Vec<CosmosMsg>, StakingError> {
        unimplemented!()
    }
}

#[cfg(feature = "osmosis")]
fn query_pool_data(deps: Deps, pool_id: u64) -> StdResult<Pool> {
    let res = QueryPoolRequest { pool_id }.query(&deps.querier).unwrap();

    let pool = Pool::try_from(res.pool.unwrap()).unwrap();
    Ok(pool)
}

#[cfg(feature = "osmosis")]
fn compute_osmo_share_out_amount(
    pool_assets: &[OsmoCoin],
    deposits: &[Uint128; 2],
    total_share: Uint128,
) -> StdResult<Uint128> {
    // ~ source: terraswap contract ~
    // min(1, 2)
    // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_1))
    // == deposit_0 * total_share / pool_0
    // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
    // == deposit_1 * total_share / pool_1
    let share_amount_out = std::cmp::min(
        deposits[0].multiply_ratio(
            total_share,
            pool_assets[0].amount.parse::<Uint128>().unwrap(),
        ),
        deposits[1].multiply_ratio(
            total_share,
            pool_assets[1].amount.parse::<Uint128>().unwrap(),
        ),
    );

    Ok(share_amount_out)
}

#[cfg(feature = "osmosis")]
fn assert_slippage_tolerance(
    slippage_tolerance: &Option<Decimal>,
    deposits: &[Uint128; 2],
    pool_assets: &[OsmoCoin],
) -> Result<(), StakingError> {
    if let Some(slippage_tolerance) = *slippage_tolerance {
        let slippage_tolerance: Decimal256 = slippage_tolerance.into();
        if slippage_tolerance > Decimal256::one() {
            return Err(StakingError::Std(StdError::generic_err(
                "slippage_tolerance cannot bigger than 1",
            )));
        }

        let one_minus_slippage_tolerance = Decimal256::one() - slippage_tolerance;
        let deposits: [Uint256; 2] = [deposits[0].into(), deposits[1].into()];
        let pools: [Uint256; 2] = [
            pool_assets[0].amount.parse::<Uint256>().unwrap(),
            pool_assets[1].amount.parse::<Uint256>().unwrap(),
        ];

        // Ensure each prices are not dropped as much as slippage tolerance rate
        if Decimal256::from_ratio(deposits[0], deposits[1]) * one_minus_slippage_tolerance
            > Decimal256::from_ratio(pools[0], pools[1])
            || Decimal256::from_ratio(deposits[1], deposits[0]) * one_minus_slippage_tolerance
                > Decimal256::from_ratio(pools[1], pools[0])
        {
            return Err(StakingError::MaxSlippageAssertion(
                slippage_tolerance.to_string(),
                OSMOSIS.to_owned(),
            ));
        }
    }

    Ok(())
}
