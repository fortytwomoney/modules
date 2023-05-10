use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use cosmwasm_std::{DepsMut, Env, Response};

/// Unused for now but provided here as an example
/// Contract version is migrated automatically
/// Abstract handles the version checks. https://github.com/AbstractSDK/contracts/blob/b58b8cb5b58b9325c9efe17e9ae28b68ee08a045/packages/abstract-app/src/endpoints/migrate.rs#L30-L51 
pub fn migrate_handler(
    _deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    _msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    Ok(Response::default())
}

#[cfg(test)]
mod test {}
