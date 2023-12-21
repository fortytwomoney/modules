use abstract_core::objects::AccountId;
use abstract_cw_staking::{interface::CwStakingAdapter, CW_STAKING};
use abstract_interface::{Abstract, ManagerQueryFns};
use abstract_interface::{AbstractAccount, AppDeployer};
use abstract_sdk::core::app;
use abstract_sdk::core::app::BaseExecuteMsg;
use cw_orch::{interface, prelude::*};

/*use boot_core::{
    contract, BootError, BootExecute, Contract, ContractWrapper, CwEnv, IndexResponse, TxResponse,
};
*/
use cosmwasm_std::{Coin, Empty};

use abstract_core::app::MigrateMsg;

use crate::msg::{AutocompounderExecuteMsg, AUTOCOMPOUNDER_ID, *};

/// Contract wrapper for deploying with BOOT
#[interface(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
pub struct AutocompounderApp;

impl<Chain: CwEnv> AppDeployer<Chain> for AutocompounderApp<Chain> {}

impl<Chain: CwEnv> Uploadable for AutocompounderApp<Chain> {
    fn wrapper(&self) -> <Mock as TxHandler>::ContractSource {
        Box::new(
            ContractWrapper::new_with_empty(
                crate::contract::execute,
                crate::contract::instantiate,
                crate::contract::query,
            )
            .with_reply(crate::contract::reply),
        )
    }
    fn wasm(&self) -> WasmPath {
        artifacts_dir_from_workspace!()
            .find_wasm_path("autocompounder")
            .unwrap()
    }
}

impl<Chain: CwEnv> AutocompounderApp<Chain>
where
    TxResponse<Chain>: IndexResponse,
{
    /// Temporary helper to execute the app explicitly
    pub fn execute_app(
        &self,
        execute_msg: AutocompounderExecuteMsg,
        coins: Option<&[Coin]>,
    ) -> Result<TxResponse<Chain>, CwOrchError> {
        self.execute(&app::ExecuteMsg::Module(execute_msg), coins)
    }

    /// Temporary helper to execute the app base explicitly
    pub fn execute_base(
        &self,
        execute_msg: BaseExecuteMsg,
        coins: Option<&[Coin]>,
    ) -> Result<TxResponse<Chain>, CwOrchError> {
        self.execute(&app::ExecuteMsg::Base(execute_msg), coins)
    }
}

pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub staking: CwStakingAdapter<Chain>,
    pub autocompounder: AutocompounderApp<Chain>,
}

impl<Chain: CwEnv> Vault<Chain> {
    pub fn new(abstract_: &Abstract<Chain>, account_id: AccountId) -> anyhow::Result<Self> {
        let chain = abstract_.ans_host.get_chain();
        let account = AbstractAccount::new(abstract_, Some(account_id.clone()));
        let staking = CwStakingAdapter::new(CW_STAKING, chain.clone());
        let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER_ID, chain.clone());

        if account.manager.is_module_installed(CW_STAKING)? {
            let cw_staking_address = account
                .manager
                .module_info(CW_STAKING)?
                .ok_or(anyhow::anyhow!("Could not find cw-staking module on Account {}", account_id))?
                .address;
            staking.set_address(&cw_staking_address);
        } else {
            return Err(anyhow::anyhow!("Cw-staking module not installed on Account {}", account_id));
        }
        if account.manager.is_module_installed(AUTOCOMPOUNDER_ID)? {
            let autocompounder_address = account
                .manager
                .module_info(AUTOCOMPOUNDER_ID)?
                .ok_or(anyhow::anyhow!("Could not find autocompounder module on Account {}", account_id))?
                .address;
            autocompounder.set_address(&autocompounder_address);
        } else {
            return Err(anyhow::anyhow!("Autocompounder module not installed on Account {}", account_id));
        }

        Ok(Self {
            account,
            staking,
            autocompounder,
        })
    }

    /// Update the vault to have the latest versions of the modules
    pub fn update(&mut self) -> anyhow::Result<()> {
        if self.account.manager.is_module_installed(CW_STAKING)? {
            self.account.manager.upgrade_module(CW_STAKING, &Empty {})?;
        }
        if self
            .account
            .manager
            .is_module_installed(AUTOCOMPOUNDER_ID)?
        {
            let ac_versions = self
                .account
                .manager
                .module_versions(vec![AUTOCOMPOUNDER_ID.to_string()])?;
            let ac_version = ac_versions
                .versions
                .first()
                .ok_or(anyhow::anyhow!("No version"))?;

            let x = app::MigrateMsg {
                module: crate::msg::AutocompounderMigrateMsg {
                    version: ac_version.version.clone(),
                },
                base: app::BaseMigrateMsg {},
            };
            self.account.manager.upgrade_module(AUTOCOMPOUNDER_ID, &x)?;
        }
        Ok(())
    }
}
