use boot_core::networks;
use boot_core::networks::NetworkInfo;

pub mod autocompounder;
pub mod cw_staking;

pub fn parse_network(net_id: &str) -> NetworkInfo {
    match net_id {
        "uni-5" => networks::UNI_5,
        "juno-1" => networks::JUNO_1,
        "pisco-1" => networks::terra::PISCO_1,
        _ => panic!("unknown network"),
    }
}
