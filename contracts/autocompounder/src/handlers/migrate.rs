use std::borrow::BorrowMut;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::msg::AutocompounderMigrateMsg;
use crate::state::{FeeConfig, FEE_CONFIG};
use abstract_core::objects::AssetEntry;
use cosmwasm_std::{from_slice, Addr, Decimal, DepsMut, Env, Response, StdError};

/// Unused for now but provided here as an example
/// Contract version is migrated automatically
pub fn migrate_handler(
    mut deps: DepsMut,
    _env: Env,
    _app: AutocompounderApp,
    _msg: AutocompounderMigrateMsg,
) -> AutocompounderResult {
    update_fee_config_v0_4_3_to_v0_4_5(deps.borrow_mut())?;

    Ok(Response::default())
}

/// Vault fee structure
#[cosmwasm_schema::cw_serde]
pub struct OldFeeConfig {
    pub performance: Decimal,
    pub deposit: Decimal,
    pub withdrawal: Decimal,
    pub fee_asset: AssetEntry,
    /// Address that receives the fee commissions
    pub fee_collector_addr: Addr,
}

fn update_fee_config_v0_4_3_to_v0_4_5(
    deps: &mut DepsMut,
) -> Result<(), crate::error::AutocompounderError> {
    let data = deps
        .storage
        .get(FEE_CONFIG.as_slice())
        .ok_or_else(|| StdError::generic_err("No config"))?;
    let config_v0_4_3: OldFeeConfig =
        from_slice(data.as_slice()).map_err(|_| StdError::generic_err("Invalid config"))?;

    let fee_config = FeeConfig {
        performance: config_v0_4_3.performance,
        withdrawal: config_v0_4_3.withdrawal,
        deposit: config_v0_4_3.deposit,
        fee_collector_addr: config_v0_4_3.fee_collector_addr,
    };
    FEE_CONFIG.save(deps.storage, &fee_config)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::to_vec;
    use cosmwasm_std::{testing::mock_dependencies, Addr, Decimal};
    use speculoos::assert_that;

    type AResult = anyhow::Result<()>;

    fn set_old_fee_config(deps: DepsMut) {
        let config = OldFeeConfig {
            performance: Decimal::percent(1),
            withdrawal: Decimal::percent(1),
            deposit: Decimal::percent(1),
            fee_asset: "fee_asset".into(),
            fee_collector_addr: Addr::unchecked("fee_collector_addr"),
        };
        deps.storage
            .set(FEE_CONFIG.as_slice(), &to_vec(&config).unwrap());
    }

    #[test]
    fn fee_config_update() -> AResult {
        let mut deps = mock_dependencies();
        set_old_fee_config(deps.as_mut());

        let _resp = update_fee_config_v0_4_3_to_v0_4_5(&mut deps.as_mut()).unwrap();
        let config = FEE_CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.fee_collector_addr).is_equal_to(Addr::unchecked("fee_collector_addr"));
        Ok(())
    }
}
