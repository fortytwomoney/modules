use crate::autocompounder::AutocompounderApp;
use crate::{get_module_address, is_module_installed};
use abstract_boot::AbstractAccount;
use abstract_core::app;
use boot_core::BootEnvironment;
use boot_core::*;
use cosmwasm_std::Empty;
use cw_staking::boot::CwStakingApi;
use cw_staking::CW_STAKING;
use forty_two::autocompounder::AUTOCOMPOUNDER;

pub struct Vault<Chain: BootEnvironment> {
    pub account: AbstractAccount<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub autocompounder: AutocompounderApp<Chain>,
}

impl<Chain: BootEnvironment> Vault<Chain> {
    pub fn new(chain: Chain, account_id: Option<u32>) -> anyhow::Result<Self> {
        let account = AbstractAccount::new(chain.clone(), account_id);
        let staking = CwStakingApi::new(CW_STAKING, chain.clone());
        let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain);

        if account_id.is_some() {
            if is_module_installed(&account, CW_STAKING)? {
                let cw_staking_address = get_module_address(&account, CW_STAKING)?;
                staking.set_address(&cw_staking_address);
            }
            if is_module_installed(&account, AUTOCOMPOUNDER)? {
                let autocompounder_address = get_module_address(&account, AUTOCOMPOUNDER)?;
                autocompounder.set_address(&autocompounder_address);
            }
        }

        Ok(Self {
            account,
            staking,
            autocompounder,
        })
    }

    /// Update the vault to have the latest versions of the modules
    pub fn update(&mut self) -> anyhow::Result<()> {
        if is_module_installed(&self.account, CW_STAKING)? {
            self.account.manager.upgrade_module(CW_STAKING, &Empty {})?;
        }
        if is_module_installed(&self.account, AUTOCOMPOUNDER)? {
            let x = app::MigrateMsg {
                module: forty_two::autocompounder::AutocompounderMigrateMsg {},
                base: app::BaseMigrateMsg {},
            };
            self.account.manager.upgrade_module(AUTOCOMPOUNDER, &x)?;
        }
        Ok(())
    }
}
