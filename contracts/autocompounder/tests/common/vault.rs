use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_interface::{Abstract, AbstractAccount};
use autocompounder::interface::AutocompounderApp;
use cw_orch::environment::CwEnv;
use cw_plus_interface::cw20_base::Cw20Base;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: Cw20Base<Chain>,
    pub staking: CwStakingAdapter<Chain>,
    pub dex: DexAdapter<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}
