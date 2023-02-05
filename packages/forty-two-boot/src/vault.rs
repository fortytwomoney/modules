use abstract_boot::OS;
use abstract_os::app;
use boot_core::BootEnvironment;
use cosmwasm_std::{Empty};
use forty_two::autocompounder::{AUTOCOMPOUNDER};
use forty_two::cw_staking::CW_STAKING;
use crate::autocompounder::AutocompounderApp;
use crate::cw_staking::CwStakingApi;
use crate::{get_module_address, is_module_installed};
use boot_core::prelude::*;

pub struct Vault<Chain: BootEnvironment> {
    chain: Chain,
    pub os: OS<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub autocompounder: AutocompounderApp<Chain>
}

impl<Chain: BootEnvironment> Vault<Chain> {
    pub fn new(chain: Chain, os_id: Option<u32>) -> anyhow::Result<Self> {
        let os = OS::new(chain.clone(), os_id);
        let staking = CwStakingApi::new(CW_STAKING, chain.clone());
        let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain.clone());

        if os_id.is_some() {
            if is_module_installed(&os, CW_STAKING)? {
                let cw_staking_address = get_module_address(&os, CW_STAKING)?;
                staking.set_address(&cw_staking_address);
            }
            if is_module_installed(&os, AUTOCOMPOUNDER)? {
                let autocompounder_address = get_module_address(&os, AUTOCOMPOUNDER)?;
                autocompounder.set_address(&autocompounder_address);
            }
        }

        Ok(Self { chain, os, staking, autocompounder })
    }

    /// Update the vault to have the latest versions of the modules
    pub fn update(&mut self) -> Result<(), BootError> {
        if is_module_installed(&self.os, CW_STAKING)? {
            self.os.manager.upgrade_module(CW_STAKING, &Empty {})?;
        }
        if is_module_installed(&self.os, AUTOCOMPOUNDER)? {
            let x = app::MigrateMsg {
                app: forty_two::autocompounder::AutocompounderMigrateMsg {},
                base: app::BaseMigrateMsg {}
            };
            self.os.manager.upgrade_module(AUTOCOMPOUNDER, &x)?;
        }
        Ok(())
    }
}

