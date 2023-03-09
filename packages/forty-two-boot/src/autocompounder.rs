use abstract_os::app;
use abstract_os::app::BaseExecuteMsg;
use boot_core::{boot_contract, BootExecute};
use boot_core::{BootEnvironment, BootError, Contract, IndexResponse, TxResponse};
use cosmwasm_std::{Addr, Coin};
use forty_two::autocompounder::{
    AutocompounderExecuteMsg, AutocompounderInstantiateMsg, AutocompounderMigrateMsg,
    AutocompounderQueryMsg, AUTOCOMPOUNDER,
};

type AppInstantiateMsg = app::InstantiateMsg<AutocompounderInstantiateMsg>;
type AppExecuteMsg = app::ExecuteMsg<AutocompounderExecuteMsg>;
type AppQueryMsg = app::QueryMsg<AutocompounderQueryMsg>;
type AppMigrateMsg = app::MigrateMsg<AutocompounderMigrateMsg>;

/// Contract wrapper for deploying with BOOT
#[boot_contract(AppInstantiateMsg, AppExecuteMsg, AppQueryMsg, AppMigrateMsg)]
pub struct AutocompounderApp<Chain>;

impl<Chain: BootEnvironment> AutocompounderApp<Chain>
where
    TxResponse<Chain>: IndexResponse,
{
    pub fn new(name: &str, chain: Chain) -> Self {
        Self(Contract::new(name, chain).with_wasm_path("autocompounder"))
    }

    pub fn load(chain: Chain, address: &Addr) -> Self {
        Self(Contract::new(AUTOCOMPOUNDER, chain).with_address(Some(address)))
    }

    /// Temporary helper to execute the app explicitly
    pub fn execute_app(
        &self,
        execute_msg: AutocompounderExecuteMsg,
        coins: Option<&[Coin]>,
    ) -> Result<TxResponse<Chain>, BootError> {
        self.execute(&app::ExecuteMsg::App(execute_msg), coins)
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
