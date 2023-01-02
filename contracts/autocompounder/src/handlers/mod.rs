pub mod execute;
pub mod instantiate;
pub mod migrate;
pub mod query;
pub mod reply;

pub use crate::handlers::{
    execute::execute_handler, execute::receive, instantiate::instantiate_handler,
    migrate::migrate_handler, query::query_handler, reply::*,
};
