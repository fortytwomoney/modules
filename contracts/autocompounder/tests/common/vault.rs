use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_interface::{Abstract, AbstractAccount};
use autocompounder::interface::AutocompounderApp;
use cw20_base::contract::AbstractCw20Base;
use cw_orch::environment::CwEnv;
use wyndex_bundle::WynDex;

pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub auto_compounder: AutocompounderApp<Chain>,
    pub vault_token: AbstractCw20Base<Chain>,
    pub staking: CwStakingAdapter<Chain>,
    pub dex: DexAdapter<Chain>,
    pub wyndex: WynDex,
    pub abstract_core: Abstract<Chain>,
}
