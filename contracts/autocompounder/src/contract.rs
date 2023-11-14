use crate::dependencies::AUTOCOMPOUNDER_DEPS;
use crate::error::AutocompounderError;
use crate::handlers::{self};
use crate::msg::{
    AutocompounderExecuteMsg, AutocompounderInstantiateMsg, AutocompounderMigrateMsg,
    AutocompounderQueryMsg, AUTOCOMPOUNDER_ID,
};
use abstract_app::export_endpoints;
use abstract_app::AppContract;
use cosmwasm_std::Response;
use cw20::Cw20ReceiveMsg;

// As an app writer, the only changes necessary to this file are with the handlers and API dependencies on the `AUTOCOMPOUNDER_APP` const.
pub type AutocompounderApp = AppContract<
    AutocompounderError,
    AutocompounderInstantiateMsg,
    AutocompounderExecuteMsg,
    AutocompounderQueryMsg,
    AutocompounderMigrateMsg,
    Cw20ReceiveMsg,
    cosmwasm_std::Empty,
>;

pub type AutocompounderResult<T = Response> = Result<T, AutocompounderError>;

/// The initial version of the app, which will use the package version if not altered
pub const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Expected replies
pub const INSTANTIATE_REPLY_ID: u64 = 0u64;
pub const LP_PROVISION_REPLY_ID: u64 = 1u64;
pub const LP_COMPOUND_REPLY_ID: u64 = 2u64;
pub const SWAPPED_REPLY_ID: u64 = 3u64;
pub const CP_PROVISION_REPLY_ID: u64 = 4u64;
pub const LP_WITHDRAWAL_REPLY_ID: u64 = 5u64;
pub const FEE_SWAPPED_REPLY: u64 = 6u64;
pub const LP_FEE_WITHDRAWAL_REPLY_ID: u64 = 7u64;

/// Used as the foundation for building your app.
/// All entrypoints are executed through this const (`instantiate`, `query`, `execute`, `migrate`)
pub const AUTOCOMPOUNDER_APP: AutocompounderApp =
    AutocompounderApp::new(AUTOCOMPOUNDER_ID, MODULE_VERSION, None)
        .with_instantiate(handlers::instantiate_handler)
        .with_query(handlers::query_handler)
        .with_execute(handlers::execute_handler)
        .with_migrate(handlers::migrate_handler)
        .with_replies(&[
            (INSTANTIATE_REPLY_ID, handlers::instantiate_reply),
            (LP_PROVISION_REPLY_ID, handlers::lp_provision_reply),
            (LP_WITHDRAWAL_REPLY_ID, handlers::lp_withdrawal_reply),
            (LP_COMPOUND_REPLY_ID, handlers::lp_compound_reply),
            (SWAPPED_REPLY_ID, handlers::swapped_reply),
            (CP_PROVISION_REPLY_ID, handlers::compound_lp_provision_reply),
        ])
        .with_receive(handlers::receive)
        .with_dependencies(AUTOCOMPOUNDER_DEPS);

// Export the endpoints for this contract
#[cfg(feature = "export")]
export_endpoints!(AUTOCOMPOUNDER_APP, AutocompounderApp);
