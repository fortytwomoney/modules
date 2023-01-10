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
use astroport::generator::{
    Cw20HookMsg, ExecuteMsg as GeneratorExecuteMsg, QueryMsg as GeneratorQueryMsg,
};
use cw_asset::{AssetInfo};
use forty_two::cw_staking::{Claim, StakingInfoResponse};

pub const ASTROPORT: &str = "astroport";

// TODO: use optional values here?
#[derive(Clone, Debug)]
pub struct Astroport {
    lp_token: LpToken,
    lp_token_address: Addr,
    generator_contract_address: Addr,
    astro_token: AssetInfo,
}

impl Default for Astroport {
    fn default() -> Self {
        Self { lp_token: Default::default(), lp_token_address: Addr::unchecked(""), generator_contract_address: Addr::unchecked(""), astro_token: cw_asset::AssetInfoBase::native("") }
    }
}

pub const ASTRO_TOKEN: &str = "astro";

// Data that's retrieved from ANS
// - LP token address, based on provided LP token
// - Generator address = staking_address
impl Identify for Astroport {
    fn name(&self) -> &'static str {
        ASTROPORT
    }
}

impl CwStaking for Astroport {
    // get the relevant data for Junoswap staking
    fn fetch_data(
        &mut self,
        deps: Deps,
        ans_host: &AnsHost,
        lp_token: AssetEntry,
    ) -> StdResult<()> {
        self.generator_contract_address =
            self.staking_contract_address(deps, ans_host, &lp_token.clone().into())?;

        let AssetInfo::Cw20(token_addr) = lp_token.resolve(&deps.querier, ans_host)? else {
                return Err(StdError::generic_err("expected CW20 as LP token for staking."));
            };
        self.lp_token_address = token_addr;
        self.lp_token = LpToken::try_from(lp_token)?;
        Ok(())
    }

    fn stake(&self, _deps: Deps, amount: Uint128) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = to_binary(&Cw20HookMsg::Deposit {})?;
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.lp_token_address.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: self.generator_contract_address.to_string(),
                amount,
                msg,
            })?,
            funds: vec![],
        })])
    }

    fn unstake(&self, _deps: Deps, amount: Uint128) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = GeneratorExecuteMsg::Withdraw {
            lp_token: self.lp_token_address.to_string(),
            amount,
        };
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.generator_contract_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn claim(&self, _deps: Deps) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = GeneratorExecuteMsg::ClaimRewards {
            lp_tokens: vec![self.lp_token_address.clone().into()],
        };

        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.generator_contract_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn query_info(&self, querier: &QuerierWrapper) -> StdResult<StakingInfoResponse> {
        let stake_info_resp: cw20_stake::state::Config = querier.query_wasm_smart(
            self.generator_contract_address.clone(),
            &GeneratorQueryMsg::Config {  },
        )?;
        Ok(StakingInfoResponse {
            staking_contract_address: self.generator_contract_address.clone(),
            staking_token: AssetInfo::Cw20(stake_info_resp.token_address),
            unbonding_period: stake_info_resp.unstaking_duration,
            max_claims: Some(cw20_stake::state::MAX_CLAIMS as u32),
        })
    }

    fn query_staked(&self, querier: &QuerierWrapper, staker: Addr) -> StdResult<Uint128> {
        let stake_balance: cw20_stake::msg::StakedBalanceAtHeightResponse = querier
            .query_wasm_smart(
                self.generator_contract_address.clone(),
                &cw20_stake::msg::QueryMsg::StakedBalanceAtHeight {
                    address: staker.into_string(),
                    height: None,
                },
            )?;
        Ok(stake_balance.balance)
    }

    fn query_unbonding(&self, querier: &QuerierWrapper, staker: Addr) -> StdResult<Vec<Claim>> {
        let claims: cw20_stake::msg::ClaimsResponse = querier.query_wasm_smart(
            self.generator_contract_address.clone(),
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
