use super::astroport::Astroport;
use abstract_boot::{Abstract, CwStakingApi, DexApi, OS};
use boot_core::BootEnvironment;
use boot_cw_plus::Cw20;
use forty_two_boot::autocompounder::AutocompounderApp;

pub struct Vault<Chain: BootEnvironment> {
    pub os: OS<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub dex: DexApi<Chain>,
    pub astroport: Astroport,
    pub abstract_os: Abstract<Chain>,
}
