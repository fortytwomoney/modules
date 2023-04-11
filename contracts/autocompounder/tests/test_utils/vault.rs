use abstract_boot::boot_core::CwEnv;
use abstract_boot::{Abstract, AbstractAccount};
use abstract_cw_staking_api::boot::CwStakingApi;
use abstract_dex_api::boot::DexApi;
use boot_cw_plus::Cw20;
use forty_two_boot::autocompounder::AutocompounderApp;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub dex: DexApi<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}
