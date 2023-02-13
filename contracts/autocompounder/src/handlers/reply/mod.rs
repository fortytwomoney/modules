mod instantiate;
mod lp_provision;
mod lp_compound;
mod lp_withdrawal;
mod swapped;
mod compound_lp_provision;
mod fee_swapped;

use abstract_sdk::{
    ModuleInterface,
    os::objects::AnsAsset
};
use cosmwasm_std::{CosmosMsg, Deps, StdResult};
use cw_utils::Duration;
pub use instantiate::instantiate_reply;
pub use lp_provision::lp_provision_reply;
pub use lp_compound::lp_compound_reply;
pub use lp_withdrawal::lp_withdrawal_reply;
pub use swapped::swapped_reply;
pub use compound_lp_provision::compound_lp_provision_reply;
pub use fee_swapped::fee_swapped_reply;
use forty_two::cw_staking::{CW_STAKING, CwStakingAction, CwStakingExecuteMsg};
use crate::contract::AutocompounderApp;


// TODO: move to cw_staking SDK
fn stake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    asset: AnsAsset,
    unbonding_period: Option<Duration>,
) -> StdResult<CosmosMsg> {
    let modules = app.modules(deps);
    modules.api_request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Stake {
                staking_token: asset,
                unbonding_period,
            },
        },
    )
}
