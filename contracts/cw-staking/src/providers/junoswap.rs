use crate::error::StakingError;
use crate::traits::cw_staking::CwStaking;
use crate::traits::identify::Identify;
use abstract_sdk::helpers::cosmwasm_std::wasm_smart_query;
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Deps, Querier, QuerierWrapper, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use cw20_junoswap::Denom;
use cw20_stake::msg::{ExecuteMsg as StakeCw20ExecuteMsg, ReceiveMsg};
use cw_asset::{Asset, AssetInfo};
use forty_two::cw_staking::{Claim, StakingInfoResponse};

pub const JUNOSWAP: &str = "junoswap";
// Source https://github.com/wasmswap/wasmswap-contracts
pub struct JunoSwap {}

impl Identify for JunoSwap {
    fn over_ibc(&self) -> bool {
        false
    }
    fn name(&self) -> &'static str {
        JUNOSWAP
    }
}

impl CwStaking for JunoSwap {
    fn stake(
        &self,
        _deps: Deps,
        staking_address: Addr,
        asset: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = to_binary(&ReceiveMsg::Stake {})?;
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: asset.info.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: staking_address.into(),
                amount: asset.amount,
                msg,
            })?,
            funds: vec![],
        })])
    }

    fn unstake(
        &self,
        _deps: Deps,
        staking_address: Addr,
        amount: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = StakeCw20ExecuteMsg::Unstake {
            amount: amount.amount,
        };
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn claim(&self, _deps: Deps, staking_address: Addr) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = StakeCw20ExecuteMsg::Claim {};

        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn query_info(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
    ) -> StdResult<StakingInfoResponse> {
        let stake_info_resp: cw20_stake::state::Config = querier.query_wasm_smart(
            staking_address.clone(),
            &cw20_stake::msg::QueryMsg::GetConfig {},
        )?;
        Ok(StakingInfoResponse {
            staking_contract_address: staking_address,
            staking_token: AssetInfo::Cw20(stake_info_resp.token_address),
            unbonding_period: stake_info_resp.unstaking_duration,
            max_claims: Some(cw20_stake::state::MAX_CLAIMS as u32),
        })
    }

    fn query_staked(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
        staker: Addr,
    ) -> StdResult<Uint128> {
        let stake_balance: cw20_stake::msg::StakedBalanceAtHeightResponse = querier
            .query_wasm_smart(
                staking_address,
                &cw20_stake::msg::QueryMsg::StakedBalanceAtHeight {
                    address: staker.into_string(),
                    height: None,
                },
            )?;
        Ok(stake_balance.balance)
    }

    fn query_unbonding(
        &self,
        querier: &QuerierWrapper,
        staking_address: Addr,
        staker: Addr,
    ) -> StdResult<Vec<Claim>> {
        let claims: cw20_stake::msg::ClaimsResponse = querier.query_wasm_smart(
            staking_address,
            &cw20_stake::msg::QueryMsg::Claims {
                address: staker.into_string(),
            },
        )?;
        let claims = claims
            .claims
            .iter()
            .map(|claim| Claim {
                amount: claim.amount,
                claimable_at: claim.release_at,
            })
            .collect();
        Ok(claims)
    }
}
