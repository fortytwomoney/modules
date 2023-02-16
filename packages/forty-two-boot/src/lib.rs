use abstract_boot::OS;
use abstract_os::manager::QueryMsgFns;
use boot_core::{BootEnvironment, networks};
use boot_core::networks::NetworkInfo;
use cosmwasm_std::Addr;

pub mod autocompounder;
pub mod vault;

pub fn parse_network(net_id: &str) -> NetworkInfo {
    match net_id {
        "uni-5" => networks::UNI_5,
        "juno-1" => networks::JUNO_1,
        "pisco-1" => networks::terra::PISCO_1,
        _ => panic!("unknown network"),
    }
}

/// TODO: abstract-boot
pub fn get_module_address<Chain: BootEnvironment>(
    os: &OS<Chain>,
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
    os: &OS<Chain>,
    module_id: &str,
) -> anyhow::Result<bool> {
    let module_infos = os.manager.module_infos(None, None)?.module_infos;
    Ok(module_infos
        .iter()
        .any(|module_info| module_info.id == module_id))
}
