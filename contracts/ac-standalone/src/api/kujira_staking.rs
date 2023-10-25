use cosmwasm_std::{Addr, Coin};
use cw_asset::Asset;

// TODO: use optional values here?
#[derive(Clone, Debug)]
pub struct Kujira {
    #[allow(unused)]
    lp_token: Asset,
    #[allow(unused)]
    lp_token_denom: Addr,
    #[allow(unused)]
    staking_contract_address: Addr,
}

impl Default for Kujira {
    fn default() -> Self {
        Self {
            lp_token: Default::default(),
            lp_token_denom: Addr::unchecked(""),
            staking_contract_address: Addr::unchecked(""),
        }
    }
}

impl Identify for Kujira {
    fn name(&self) -> &'static str {
        KUJIRA
    }
    fn is_available_on(&self, chain_name: &str) -> bool {
        AVAILABLE_CHAINS.contains(&chain_name)
    }
}

#[cfg(feature = "kujira")]
use ::{
    abstract_sdk::{
        core::objects::{AnsEntryConvertor, AssetEntry},
        feature_objects::AnsHost,
        AbstractSdkResult, Resolve,
    },
    abstract_staking_adapter_traits::query_responses::{
        RewardTokensResponse, StakeResponse, StakingInfoResponse, UnbondingResponse,
    },
    abstract_staking_adapter_traits::{CwStakingCommand, CwStakingError},
    cosmwasm_std::{
        to_binary, wasm_execute, CosmosMsg, Deps, Env, QuerierWrapper, StdError, Uint128,
    },
    cw20::Cw20ExecuteMsg,
    cw_asset::AssetInfo,
    cw_utils::Duration,
    kujira::{
        bow::{self, staking as BowStaking},
        fin,
    },
};

#[cfg(feature = "kujira")]
impl CwStakingCommand for Kujira {
    fn fetch_data(
        &mut self,
        deps: Deps,
        _env: Env,
        ans_host: &AnsHost,
        lp_token: AssetEntry,
    ) -> AbstractSdkResult<()> {
        self.staking_contract_address = self.staking_contract_address(deps, ans_host, &lp_token)?;

        if let AssetInfo::Native(denom) = lp_token {
            self.lp_token_denom = denom;
        } else {
            return Err(StdError::generic_err("expected native token as LP token for staking - Kujira only supports native tokens"));
        }
        self.lp_token = AnsEntryConvertor::new(lp_token).lp_token()?;
        Ok(())
    }

    fn stake(
        &self,
        _deps: Deps,
        amount: Uint128,
        _unbonding_period: Option<Duration>,
    ) -> Result<Vec<CosmosMsg>, CwStakingError> {
        let msg = BowStaking::ExecuteMsg::Stake { addr: None };
        Ok(vec![wasm_execute(
            self.staking_contract_address,
            &msg,
            vec![Coin {
                amount,
                denom: self.lp_token_denom.into(),
            }],
        )?
        .into()])
    }

    fn unstake(
        &self,
        _deps: Deps,
        amount: Uint128,
        _unbonding_period: Option<Duration>,
    ) -> Result<Vec<CosmosMsg>, CwStakingError> {
        let msg = BowStaking::ExecuteMsg::Withdraw {
            amount: Coin {
                denom: self.lp_token_denom,
                amount,
            },
        };
        Ok(vec![wasm_execute(
            self.staking_contract_address,
            &msg,
            vec![],
        )?
        .into()])
    }

    fn claim(&self, _deps: Deps) -> Result<Vec<CosmosMsg>, CwStakingError> {
        Ok(vec![])
    }

    fn claim_rewards(&self, _deps: Deps) -> Result<Vec<CosmosMsg>, CwStakingError> {
        let msg = BowStaking::ExecuteMsg::Claim {
            denom: self.lp_token_denom,
        };
        Ok(vec![wasm_execute(
            self.staking_contract_address,
            &msg,
            vec![],
        )?
        .into()])
    }

    fn query_info(&self, querier: &QuerierWrapper) -> Result<StakingInfoResponse, CwStakingError> {
        let lp_token = AssetInfo::Native(self.lp_token_denom);

        Ok(StakingInfoResponse {
            staking_contract_address: self.staking_contract_address.clone(),
            staking_token: lp_token,
            unbonding_periods: None,
            max_claims: None,
        })
    }

    fn query_staked(
        &self,
        querier: &QuerierWrapper,
        staker: Addr,
        _unbonding_period: Option<Duration>,
    ) -> Result<StakeResponse, CwStakingError> {
        let stake_response: BowStaking::StakeResponse = querier
            .query_wasm_smart(
                self.staking_contract_address.clone(),
                &BowStaking::QueryMsg::Stake {
                    denom: self.lp_token_denom,
                    addr: staker,
                },
            )
            .map_err(|e| {
                StdError::generic_err(format!(
                    "Failed to query staked balance on {} for {}. Error: {:?}",
                    self.name(),
                    staker,
                    e
                ))
            })?;
        Ok(StakeResponse {
            amount: stake_response.amount,
        })
    }

    fn query_unbonding(
        &self,
        _querier: &QuerierWrapper,
        _staker: Addr,
    ) -> Result<UnbondingResponse, CwStakingError> {
        Ok(UnbondingResponse { claims: vec![] })
    }

    fn query_rewards(
        &self,
        querier: &QuerierWrapper,
    ) -> Result<
        abstract_staking_adapter_traits::query_responses::RewardTokensResponse,
        CwStakingError,
    > {
        let reward_info: BowStaking::IncentivesResponse = querier
            .query_wasm_smart(
                self.staking_contract_address,
                &BowStaking::QueryMsg::Incentives {
                    denom: self.lp_token_denom,
                    start_after: None,
                    limit: None,
                },
            )
            .map_err(|e| {
                StdError::generic_err(format!(
                    "Failed to query reward info on {} for lp token {}. Error: {:?}",
                    self.name(),
                    self.lp_token,
                    e
                ))
            })?;

        let reward_tokens = reward_info
            .incentives
            .into_iter()
            .map(|(asset, _)| {
                let token = AssetInfo::Native(asset.denom);
                Result::<_, CwStakingError>::Ok(token)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RewardTokensResponse {
            tokens: reward_tokens,
        })
    }
}