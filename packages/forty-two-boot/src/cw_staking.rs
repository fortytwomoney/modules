use boot_core::prelude::boot_contract;

use boot_core::{BootEnvironment, Contract, IndexResponse, TxResponse};

use abstract_os::api;
use cosmwasm_std::{Addr, Empty};
use forty_two::cw_staking::{CwStakingExecuteMsg, CwStakingQueryMsg, CW_STAKING};

type ApiExecuteMsg = api::ExecuteMsg<CwStakingExecuteMsg>;
type ApiQueryMsg = api::QueryMsg<CwStakingQueryMsg>;

/// Contract wrapper for interacting with BOOT
#[boot_contract(Empty, ApiExecuteMsg, ApiQueryMsg, Empty)]
pub struct CwStakingApi<Chain>;

/// implement chain-generic functions
impl<Chain: BootEnvironment> CwStakingApi<Chain>
where
    TxResponse<Chain>: IndexResponse,
{
    pub fn new(id: &str, chain: Chain) -> Self {
        Self(Contract::new(id, chain).with_wasm_path("cw_staking"))
    }

    pub fn load(chain: Chain, addr: &Addr) -> Self {
        Self(Contract::new(CW_STAKING, chain).with_address(Some(addr)))
    }
}
