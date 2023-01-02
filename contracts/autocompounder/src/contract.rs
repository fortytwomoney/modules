use abstract_app::export_endpoints;
use abstract_app::AppContract;

use cosmwasm_std::Response;

use forty_two::autocompounder::{
    AutocompounderExecuteMsg, AutocompounderInstantiateMsg, AutocompounderMigrateMsg,
    AutocompounderQueryMsg, AUTOCOMPOUNDER,
};

use crate::dependencies::AUTOCOMPOUNDER_DEPS;

use crate::error::AutocompounderError;
use crate::handlers::{self};

// As an app writer, the only changes necessary to this file are with the handlers and API dependencies on the `AUTOCOMPOUNDER_APP` const.
pub type AutocompounderApp = AppContract<
    AutocompounderError,
    AutocompounderExecuteMsg,
    AutocompounderInstantiateMsg,
    AutocompounderQueryMsg,
    AutocompounderMigrateMsg,
>;

pub type AutocompounderResult = Result<Response, AutocompounderError>;

/// The initial version of the app, which will use the package version if not altered
const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Expected replies
pub const INSTANTIATE_REPLY_ID: u64 = 0u64;
pub const LP_PROVISION_REPLY_ID: u64 = 1u64;

/// Used as the foundation for building your app.
/// All entrypoints are executed through this const (`instantiate`, `query`, `execute`, `migrate`)
const APP: AutocompounderApp = AutocompounderApp::new(AUTOCOMPOUNDER, MODULE_VERSION, None)
    .with_instantiate(handlers::instantiate_handler)
    .with_query(handlers::query_handler)
    .with_execute(handlers::execute_handler)
    .with_migrate(handlers::migrate_handler)
    .with_replies(&[
        (INSTANTIATE_REPLY_ID, handlers::instantiate_reply),
        (LP_PROVISION_REPLY_ID, handlers::lp_provision_reply),
    ])
    .with_dependencies(AUTOCOMPOUNDER_DEPS);

// Export the endpoints for this contract
export_endpoints!(APP, AutocompounderApp);
