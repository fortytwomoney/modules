use abstract_cw_staking::CW_STAKING_ADAPTER_ID;
use abstract_dex_adapter::DEX_ADAPTER_ID;
use abstract_cw_staking::contract::CONTRACT_VERSION as CW_STAKING_ADAPTER_VERSION;
use abstract_dex_adapter::contract::CONTRACT_VERSION as DEX_ADAPTER_VERSION;
use abstract_sdk::core::objects::dependency::StaticDependency;

const DEX_ADAPTER_DEP: StaticDependency = StaticDependency::new(DEX_ADAPTER_ID, &[DEX_ADAPTER_VERSION]);

const CW_STAKING_DEP: StaticDependency = StaticDependency::new(CW_STAKING_ADAPTER_ID, &[CW_STAKING_ADAPTER_VERSION]);

/// Dependencies for the app
pub const AUTOCOMPOUNDER_DEPS: &[StaticDependency] = &[DEX_ADAPTER_DEP, CW_STAKING_DEP];

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
