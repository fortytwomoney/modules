use crate::error::StakingError;
use crate::traits::cw_staking_adapter::CwStakingAdapter;
use abstract_sdk::base::features::AbstractNameService;
use abstract_sdk::os::objects::AssetEntry;
use abstract_sdk::Execution;
use cosmwasm_std::{DepsMut, SubMsg};
use forty_two::cw_staking::CwStakingAction;

impl<T> LocalCwStaking for T where T: AbstractNameService + Execution {}

/// Trait for dispatching *local* staking actions to the appropriate provider
/// Resolves the required data for that provider
pub trait LocalCwStaking: AbstractNameService + Execution {
    /// resolve the provided dex action on a local dex
    fn resolve_staking_action(
        &self,
        deps: DepsMut,
        action: CwStakingAction,
        mut provider: Box<dyn CwStakingAdapter>,
    ) -> Result<SubMsg, StakingError> {
        let staking_asset = staking_asset_from_action(&action);
        provider.fetch_data(deps.as_ref(), &self.ans_host(deps.as_ref())?, staking_asset)?;

        let msgs = match action {
            CwStakingAction::Stake {
                staking_token,
                unbonding_period,
            } => provider.stake(deps.as_ref(), staking_token.amount, unbonding_period)?,
            CwStakingAction::Unstake {
                staking_token,
                unbonding_period,
            } => provider.unstake(deps.as_ref(), staking_token.amount, unbonding_period)?,
            CwStakingAction::ClaimRewards { staking_token: _ } => provider.claim(deps.as_ref())?,
        };

        self.executor(deps.as_ref())
            .execute(msgs)
            .map(SubMsg::new)
            .map_err(Into::into)
    }
}

#[inline(always)]
fn staking_asset_from_action(action: &CwStakingAction) -> AssetEntry {
    match action {
        CwStakingAction::Stake { staking_token, .. } => staking_token.name.clone(),
        CwStakingAction::Unstake { staking_token, .. } => staking_token.name.clone(),
        CwStakingAction::ClaimRewards { staking_token } => staking_token.clone(),
    }
}
