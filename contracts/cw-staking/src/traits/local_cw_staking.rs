use abstract_sdk::{base::features::AbstractNameService, os::objects::AnsAsset};

use abstract_sdk::Execution;
use cosmwasm_std::{CosmosMsg, Deps, DepsMut, ReplyOn, SubMsg};

use crate::error::StakingError;
use crate::traits::cw_staking::CwStaking;
use abstract_sdk::os::objects::AssetEntry;
use forty_two::cw_staking::CwStakingAction;

pub const STAKE_REPLY_ID: u64 = 8542;
pub const UNSTAKE_REPLY_ID: u64 = 8543;
pub const CLAIM_REPLY_ID: u64 = 8546;

impl<T> LocalCwStaking for T where T: AbstractNameService + Execution {}

/// Trait for dispatching *local* staking actions to the appropriate provider
/// Resolves ANS entries
pub trait LocalCwStaking: AbstractNameService + Execution {
    /// resolve the provided dex action on a local dex
    fn resolve_staking_action(
        &self,
        deps: DepsMut,
        action: CwStakingAction,
        exchange: &dyn CwStaking,
        with_reply: bool,
    ) -> Result<SubMsg, StakingError> {
        let (msgs, reply_id) = match action {
            CwStakingAction::Stake { staking_token } => (
                self.resolve_stake(deps.as_ref(), staking_token, exchange)?,
                STAKE_REPLY_ID,
            ),
            CwStakingAction::Unstake { staking_token } => (
                self.resolve_unstake(deps.as_ref(), staking_token, exchange)?,
                UNSTAKE_REPLY_ID,
            ),
            CwStakingAction::ClaimRewards { staking_token } => (
                self.resolve_claim(deps.as_ref(), staking_token, exchange)?,
                CLAIM_REPLY_ID,
            ),
        };
        if with_reply {
            self.executor(deps.as_ref())
                .execute_with_reply(msgs, ReplyOn::Success, reply_id)
        } else {
            self.executor(deps.as_ref()).execute(msgs).map(SubMsg::new)
        }
        .map_err(Into::into)
    }

    fn resolve_stake(
        &self,
        deps: Deps,
        staking_token: AnsAsset,
        provider: &dyn CwStaking,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let ans = self.name_service(deps);

        let staking_address =
            provider.staking_contract_address(deps, ans.host(), &staking_token.name)?;
        let staking_asset = ans.query(&staking_token)?;

        provider.stake(deps, staking_address, staking_asset)
    }

    fn resolve_unstake(
        &self,
        deps: Deps,
        staking_token: AnsAsset,
        provider: &dyn CwStaking,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let ans = self.name_service(deps);
        let staking_address =
            provider.staking_contract_address(deps, ans.host(), &staking_token.name)?;
        let staking_asset = ans.query(&staking_token)?;

        provider.unstake(deps, staking_address, staking_asset)
    }

    fn resolve_claim(
        &self,
        deps: Deps,
        staking_token_name: AssetEntry,
        provider: &dyn CwStaking,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let ans = self.name_service(deps);

        let staking_address =
            provider.staking_contract_address(deps, ans.host(), &staking_token_name)?;
        provider.claim(deps, staking_address)
    }
}
