use abstract_cw_staking::CW_STAKING;
use abstract_dex_adapter::EXCHANGE;
use abstract_sdk::core::objects::dependency::StaticDependency;
use croncat_app::contract::{CRONCAT_ID, CRONCAT_MODULE_VERSION};

const DEX_DEP: StaticDependency = StaticDependency::new(EXCHANGE, &[">=0.3.0"]);

const CW_STAKING_DEP: StaticDependency = StaticDependency::new(CW_STAKING, &[">=0.1.0"]);

const CRONCAT_DEP: StaticDependency = StaticDependency::new(CRONCAT_ID, &[CRONCAT_MODULE_VERSION]);

/// Dependencies for the app
pub const AUTOCOMPOUNDER_DEPS: &[StaticDependency] = &[DEX_DEP, CW_STAKING_DEP, CRONCAT_DEP];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependencies() {
        AUTOCOMPOUNDER_DEPS.iter().for_each(|dep| {
            dep.check().unwrap();
        });
    }
}
