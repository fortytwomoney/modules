use abstract_boot::{AbstractAccount, AppDeployer, ManagerQueryFns};
use abstract_cw_staking_api::{
    boot::CwStakingApi,
    CW_STAKING
};
use abstract_sdk::core::app;
use abstract_sdk::core::app::BaseExecuteMsg;
use boot_core::{
    BootError,
    BootExecute,
    contract,
    Contract,
    ContractWrapper,
    CwEnv,
    IndexResponse,
    TxResponse
};
use boot_core::*;
use cosmwasm_std::{
    Addr,
    Coin,
    Empty
};

use abstract_core::{
    app::MigrateMsg
};

use crate::{
    msg::{
        *,
        AUTOCOMPOUNDER,
        AutocompounderExecuteMsg
    },

};

/// Contract wrapper for deploying with BOOT
#[contract(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
pub struct AutocompounderApp<Chain>;

impl<Chain: CwEnv> AppDeployer<Chain> for AutocompounderApp<Chain> {}

impl<Chain: CwEnv> AutocompounderApp<Chain> {
    pub fn new(name: &str, chain: Chain) -> Self {
        let mut contract = Contract::new(name, chain);
        contract = contract.with_wasm_path("autocompounder").with_mock(Box::new(
            ContractWrapper::new_with_empty(
                crate::contract::execute,
                crate::contract::instantiate,
                crate::contract::query,
            )
                .with_reply(crate::contract::reply),
        ));
        Self(contract)
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
    ) -> Result<TxResponse<Chain>, BootError> {
        self.execute(&app::ExecuteMsg::Module(execute_msg), coins)
    }

    /// Temporary helper to execute the app base explicitly
    pub fn execute_base(
        &self,
        execute_msg: BaseExecuteMsg,
        coins: Option<&[Coin]>,
    ) -> Result<TxResponse<Chain>, BootError> {
        self.execute(&app::ExecuteMsg::Base(execute_msg), coins)
    }
}


/// TODO: abstract-boot
pub fn get_module_address<Chain: CwEnv>(
    account: &AbstractAccount<Chain>,
    module_id: &str,
) -> anyhow::Result<Addr> {
    let module_infos = account.manager.module_infos(None, None)?.module_infos;
    let module_info = module_infos
        .iter()
        .find(|module_info| module_info.id == module_id)
        .ok_or(anyhow::anyhow!("Module not found"))?;
    Ok(Addr::unchecked(module_info.address.clone()))
}

// TODO: abstract boot
pub fn is_module_installed<Chain: CwEnv>(
    account: &AbstractAccount<Chain>,
    module_id: &str,
) -> anyhow::Result<bool> {
    let module_infos = account.manager.module_infos(None, None)?.module_infos;
    Ok(module_infos
        .iter()
        .any(|module_info| module_info.id == module_id))
}


pub struct Vault<Chain: CwEnv> {
    pub account: AbstractAccount<Chain>,
    pub staking: CwStakingApi<Chain>,
    pub autocompounder: AutocompounderApp<Chain>,
}

impl<Chain: CwEnv> Vault<Chain> {
    pub fn new(chain: Chain, account_id: Option<u32>) -> anyhow::Result<Self> {
        let account = AbstractAccount::new(chain.clone(), account_id);
        let staking = CwStakingApi::new(CW_STAKING, chain.clone());
        let autocompounder = AutocompounderApp::new(AUTOCOMPOUNDER, chain.clone());

        if account_id.is_some() {
            if is_module_installed(&account, CW_STAKING)? {
                let cw_staking_address =  get_module_address(&account, CW_STAKING)?;
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
                module: crate::msg::AutocompounderMigrateMsg {},
                base: app::BaseMigrateMsg {},
            };
            self.account.manager.upgrade_module(AUTOCOMPOUNDER, &x)?;
        }
        Ok(())
    }
}

