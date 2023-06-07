use crate::msg::*;
use abstract_interface::AppDeployer;
use abstract_core::app::MigrateMsg;
use cw_orch::prelude::*;
use cw_orch::interface;

#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
pub struct FeeCollector;

impl<Chain: CwEnv> AppDeployer<Chain> for FeeCollector<Chain> {}

impl<Chain: CwEnv> Uploadable for FeeCollector<Chain> {

    fn wrapper(&self) -> <Mock as TxHandler>::ContractSource {
        Box::new(
            ContractWrapper::new_with_empty(
                crate::contract::execute,
                crate::contract::instantiate,
                crate::contract::query,
            )
            .with_migrate(crate::contract::migrate),
        )
    }
    fn wasm(&self) -> WasmPath {
        artifacts_dir_from_workspace!()
            .find_wasm_path("fee_collector_app")
            .unwrap()
    }
}
