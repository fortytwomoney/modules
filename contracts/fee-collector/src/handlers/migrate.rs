use crate::contract::{FeeCollectorApp, FeeCollectorResult};
use crate::msg::FeeCollectorMigrateMsg;
use abstract_sdk::AbstractResponse;
use cosmwasm_std::{DepsMut, Env};

/// Handle the app migrate msg
/// The top-level Abstract app does version checking and dispatches to this handler
pub fn migrate_handler(
    _deps: DepsMut,
    _env: Env,
    app: FeeCollectorApp,
    _msg: FeeCollectorMigrateMsg,
) -> FeeCollectorResult {
    Ok(app.response("migrate"))
}
