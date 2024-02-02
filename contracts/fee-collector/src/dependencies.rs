use abstract_dex_adapter::DEX_ADAPTER_ID;
use abstract_sdk::core::objects::dependency::StaticDependency;

// This is an example dependency on another app or adapter module
const DEX_DEP: StaticDependency = StaticDependency::new(DEX_ADAPTER_ID, &[">=0.3.0"]);

// const CW_STAKING_DEP: StaticDependency = StaticDependency::new(CW_STAKING, &[">=0.1.0"]);

/// Dependencies for the app
pub const FEE_COLLECTOR_DEPS: &[StaticDependency] = &[DEX_DEP]; // CW_STAKING_DEP];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependencies() {
        FEE_COLLECTOR_DEPS.iter().for_each(|dep| {
            dep.check().unwrap();
        });
    }
}
