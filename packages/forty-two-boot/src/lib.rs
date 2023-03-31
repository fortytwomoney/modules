use abstract_boot::{ManagerQueryFns, AbstractAccount};
use boot_core::BootEnvironment;

use cosmwasm_std::Addr;

pub mod autocompounder;
pub mod vault;

/// TODO: abstract-boot
pub fn get_module_address<Chain: BootEnvironment>(
    os: &AbstractAccount<Chain>,
    module_id: &str,
) -> anyhow::Result<Addr> {
    let module_infos = os.manager.module_infos(None, None)?.module_infos;
    let module_info = module_infos
        .iter()
        .find(|module_info| module_info.id == module_id)
        .ok_or(anyhow::anyhow!("Module not found"))?;
    Ok(Addr::unchecked(module_info.address.clone()))
}

// TODO: abstract boot
pub fn is_module_installed<Chain: BootEnvironment>(
    os: &AbstractAccount<Chain>,
    module_id: &str,
) -> anyhow::Result<bool> {
    let module_infos = os.manager.module_infos(None, None)?.module_infos;
    Ok(module_infos
        .iter()
        .any(|module_info| module_info.id == module_id))
}
