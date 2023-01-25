use crate::error::StakingError;
use crate::traits::cw_staking::CwStaking;
use crate::traits::identify::Identify;
use abstract_sdk::{
    feature_objects::AnsHost,
    os::objects::{AssetEntry, LpToken},
    Resolve,
};
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, QuerierWrapper, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use wyndex_stake::msg::{ExecuteMsg as StakeCw20ExecuteMsg, ReceiveDelegationMsg as ReceiveMsg, BondingInfoResponse};
use cw_asset::{AssetInfo, AssetInfoBase};
use cw_utils::Duration;
use forty_two::cw_staking::{
    Claim, RewardTokensResponse, StakeResponse, StakingInfoResponse, UnbondingResponse,
};

pub const WYNDEX: &str = "wyndex";

pub const WYND_TOKEN: &str = "juno>wynd";
// Source https://github.com/wasmswap/wasmswap-contracts
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WynDex {
    lp_token: LpToken,
    lp_token_address: Addr,
    staking_contract_address: Addr,
    ans_host: Addr,
}

impl Default for WynDex {
    fn default() -> Self {
        Self {
            lp_token: Default::default(),
            lp_token_address: Addr::unchecked(""),
            staking_contract_address: Addr::unchecked(""),
            ans_host: Addr::unchecked(""),
        }
    }
}

impl Identify for WynDex {
    fn name(&self) -> &'static str {
        WYNDEX
    }
}

impl CwStaking for WynDex {
    // get the relevant data for Junoswap staking
    fn fetch_data(
        &mut self,
        deps: Deps,
        ans_host: &AnsHost,
        lp_token: AssetEntry,
    ) -> StdResult<()> {
        self.staking_contract_address = self.staking_contract_address(deps, ans_host, &lp_token)?;

        let AssetInfoBase::Cw20(token_addr) = lp_token.resolve(&deps.querier, ans_host)? else {
                return Err(StdError::generic_err("expected CW20 as LP token for staking."));
            };
        self.lp_token_address = token_addr;
        self.lp_token = LpToken::try_from(lp_token)?;
        Ok(())
    }

    fn stake(
        &self,
        _deps: Deps,
        amount: Uint128,
        unbonding_period: Option<Duration>,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let unbonding_period = unwrap_unbond(self,unbonding_period)?;
        let msg = to_binary(&ReceiveMsg::Delegate { unbonding_period, delegate_as: None })?;
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.lp_token_address.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: self.staking_contract_address.to_string(),
                amount,
                msg,
            })?,
            funds: vec![],
        })])
    }

    fn unstake(
        &self,
        _deps: Deps,
        amount: Uint128,
        unbonding_period: Option<Duration>,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let unbonding_period = unwrap_unbond(self,unbonding_period)?;
        let msg = StakeCw20ExecuteMsg::Unbond { tokens: amount, unbonding_period };
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.staking_contract_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn claim(&self, _deps: Deps) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = StakeCw20ExecuteMsg::Claim {};

        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.staking_contract_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn query_info(&self, querier: &QuerierWrapper) -> StdResult<StakingInfoResponse> {
        let stake_info_resp: BondingInfoResponse = querier.query_wasm_smart(
            self.staking_contract_address.clone(),
            &wyndex_stake::msg::QueryMsg::BondingInfo {  },
        )?;

    
        

        Ok(StakingInfoResponse {
            staking_contract_address: self.staking_contract_address.clone(),
            staking_token: AssetInfo::Cw20(stake_info_resp.),
            unbonding_periods: Some(stake_info_resp
                .bonding
                .into_iter()
                .map(|bond_period| Duration::Time(bond_period.unbonding_period)).collect()),
            max_claims: Some(wyndex_stake::state::MAX_CLAIMS as u32),
        })
    }

    fn query_staked(&self, querier: &QuerierWrapper, staker: Addr) -> StdResult<StakeResponse> {
        let stake_balance: wyndex_stake::msg::StakedBalanceAtHeightResponse = querier
            .query_wasm_smart(
                self.staking_contract_address.clone(),
                &wyndex_stake::msg::QueryMsg::StakedBalanceAtHeight {
                    address: staker.into_string(),
                    height: None,
                },
            )?;
        Ok(StakeResponse {
            amount: stake_balance.balance,
        })
    }

    fn query_unbonding(
        &self,
        querier: &QuerierWrapper,
        staker: Addr,
    ) -> StdResult<UnbondingResponse> {
        let claims: wyndex_stake::msg::ClaimsResponse = querier.query_wasm_smart(
            self.staking_contract_address.clone(),
            &wyndex_stake::msg::QueryMsg::Claims {
                address: staker.into_string(),
            },
        )?;
        let claims = claims
            .claims
            .iter()
            .map(|claim| Claim {
                amount: claim.amount,
                claimable_at: parse_expiration(claim.release_at),
            })
            .collect();
        Ok(UnbondingResponse { claims })
    }
    fn query_reward_tokens(&self, querier: &QuerierWrapper) -> StdResult<RewardTokensResponse> {
        // hardcode as wynd token for now.
        let token = AssetEntry::new(WYND_TOKEN).resolve(
            querier,
            &AnsHost {
                address: self.ans_host.clone(),
            },
        )?;
        wyndex::stake::
        Ok(RewardTokensResponse {
            tokens: vec![token],
        })
    }
}

fn unwrap_unbond(dex: &WynDex, unbonding_period: Option<Duration>) -> Result<u64,StakingError> {
    let Some(Duration::Time(unbonding_period)) = unbonding_period else {
        if unbonding_period.is_none() {
            return Err(StakingError::UnbondingPeriodNotSet(dex.name().to_owned()));
        } else {
            return Err(StakingError::UnbondingPeriodNotSupported("height".to_owned(), dex.name().to_owned()));
        }
    };
    Ok(unbonding_period)
}

fn parse_duration(d: dao_cw_utils::Duration) -> cw_utils::Duration {
    match d {
        dao_cw_utils::Duration::Height(a) => cw_utils::Duration::Height(a),
        dao_cw_utils::Duration::Time(a) => cw_utils::Duration::Time(a),
    }
}

fn parse_expiration(d: dao_cw_utils::Expiration) -> cw_utils::Expiration {
    match d {
        dao_cw_utils::Expiration::AtHeight(a) => cw_utils::Expiration::AtHeight(a),
        dao_cw_utils::Expiration::AtTime(a) => cw_utils::Expiration::AtTime(a),
        _ => cw_utils::Expiration::Never {},
    }
}
