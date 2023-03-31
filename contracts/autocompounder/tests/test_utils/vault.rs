use abstract_boot::boot_core::BootEnvironment;
use abstract_boot::{Abstract, AbstractAccount};
use boot_cw_plus::Cw20;
use cw_staking::boot::CwStakingApi;
use dex::boot::DexApi;
use forty_two_boot::autocompounder::AutocompounderApp;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: BootEnvironment> {
    pub os: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub dex: DexApi<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}
