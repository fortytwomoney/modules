use abstract_sdk::core::objects::dependency::StaticDependency;
use cw_staking::CW_STAKING;
use dex::EXCHANGE;

const DEX_DEP: StaticDependency = StaticDependency::new(EXCHANGE, &[">=0.3.0"]);

const CW_STAKING_DEP: StaticDependency = StaticDependency::new(CW_STAKING, &[">=0.1.0"]);

/// Dependencies for the app
pub const AUTOCOMPOUNDER_DEPS: &[StaticDependency] = &[DEX_DEP, CW_STAKING_DEP];

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
