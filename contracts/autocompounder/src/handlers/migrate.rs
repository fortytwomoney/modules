use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::msg::AutocompounderMigrateMsg;
use cosmwasm_std::{DepsMut, Env, Response};
use cw2::{get_contract_version, set_contract_version};
use semver::Version;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Unused for now but provided here as an example
/// Contract version is migrated automatically
pub fn migrate_handler(
    deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    _msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    let version: Version = CONTRACT_VERSION.parse().unwrap();
    let storage_version: Version = get_contract_version(deps.storage)?.version.parse().unwrap();

    if storage_version < version {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    } else {
        return Err(AutocompounderError::InvalidContractVersion {
            storage_version: storage_version.to_string(),
            version: version.to_string(),
        });
    }
    Ok(Response::default())
}
