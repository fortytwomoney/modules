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

#[cfg(test)]
mod test {
    

    use crate::test_common::app_init;
    use crate::{contract::AUTOCOMPOUNDER_APP};
    
    
    use cw2::CONTRACT;

    use cosmwasm_std::testing::{mock_env};

    use super::*;

    #[test]
    fn test_migration_version() -> anyhow::Result<()> {
        let mut deps = app_init(false);
        let prev_version = cw2::ContractVersion {
            contract: "4t2:autocompounder".to_string(),
            version: "0.4.0".to_string(),
        };

        CONTRACT.save(deps.as_mut().storage, &prev_version)?;
        let msg = AutocompounderMigrateMsg {};
        // version of new contract is equal to the version of the contract in storage -> fail
        let _err = migrate_handler(deps.as_mut(), mock_env(), AUTOCOMPOUNDER_APP, msg.clone())
            .unwrap_err();

        let prev_version = cw2::ContractVersion {
            contract: "4t2:autocompounder".to_string(),
            version: "0.3.0".to_string(),
        };
        CONTRACT.save(deps.as_mut().storage, &prev_version)?;
        // version of new contract is greater than the version of the contract in storage -> success
        migrate_handler(deps.as_mut(), mock_env(), AUTOCOMPOUNDER_APP, msg)?;

        Ok(())
    }
}
