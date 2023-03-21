use abstract_boot::boot_core::BootEnvironment;
use abstract_boot::{Abstract, OS};
use boot_cw_plus::Cw20;
use cw_staking::boot::CwStakingApi;
use dex::boot::DexApi;
use forty_two_boot::autocompounder::AutocompounderApp;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: BootEnvironment> {
    pub os: OS<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub dex: DexApi<Chain>,
    pub wyndex: WynDex,
    pub abstract_os: Abstract<Chain>,
}
