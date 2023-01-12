pub const ROOT_USER: &str = "root_user";
pub const TEST_COIN: &str = "ucoin";

use abstract_boot::{TMintStakingApi, OS, DexApi};
use abstract_os::{
    api::InstantiateMsg, objects::gov_type::GovernanceDetails, PROXY, TENDERMINT_STAKING,
};

use boot_core::{
    prelude::{BootInstantiate, BootUpload, ContractInstance},
    Mock,
};
use cosmwasm_std::{Addr, Empty};

use abstract_boot::{
    AnsHost, Deployment, Manager, ModuleFactory, OSFactory, Proxy, VersionControl,
};

use abstract_os::{ANS_HOST, MANAGER, MODULE_FACTORY, OS_FACTORY, VERSION_CONTROL};

use cw_multi_test::ContractWrapper;
use semver::Version;

pub fn init_abstract_env(chain: &Mock) -> anyhow::Result<(Deployment<Mock>, OS<Mock>)> {
    let mut ans_host = AnsHost::new(ANS_HOST, chain.clone());
    let mut os_factory = OSFactory::new(OS_FACTORY, chain.clone());
    let mut version_control = VersionControl::new(VERSION_CONTROL, chain.clone());
    let mut module_factory = ModuleFactory::new(MODULE_FACTORY, chain.clone());
    let mut manager = Manager::new(MANAGER, chain.clone());
    let mut proxy = Proxy::new(PROXY, chain.clone());

    ans_host
        .as_instance_mut()
        .set_mock(Box::new(ContractWrapper::new_with_empty(
            ::ans_host::contract::execute,
            ::ans_host::contract::instantiate,
            ::ans_host::contract::query,
        )));

    os_factory.as_instance_mut().set_mock(Box::new(
        ContractWrapper::new_with_empty(
            ::os_factory::contract::execute,
            ::os_factory::contract::instantiate,
            ::os_factory::contract::query,
        )
        .with_reply_empty(::os_factory::contract::reply),
    ));

    module_factory.as_instance_mut().set_mock(Box::new(
        cw_multi_test::ContractWrapper::new_with_empty(
            ::module_factory::contract::execute,
            ::module_factory::contract::instantiate,
            ::module_factory::contract::query,
        )
        .with_reply_empty(::module_factory::contract::reply),
    ));

    version_control.as_instance_mut().set_mock(Box::new(
        cw_multi_test::ContractWrapper::new_with_empty(
            ::version_control::contract::execute,
            ::version_control::contract::instantiate,
            ::version_control::contract::query,
        ),
    ));

    manager
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::manager::contract::execute,
            ::manager::contract::instantiate,
            ::manager::contract::query,
        )));

    proxy
        .as_instance_mut()
        .set_mock(Box::new(cw_multi_test::ContractWrapper::new_with_empty(
            ::proxy::contract::execute,
            ::proxy::contract::instantiate,
            ::proxy::contract::query,
        )));

    // do as above for the rest of the contracts

    let deployment = Deployment {
        chain,
        version: "1.0.0".parse()?,
        ans_host,
        os_factory,
        version_control,
        module_factory,
    };

    let os_core = OS { manager, proxy };

    Ok((deployment, os_core))
}

pub(crate) type AResult = anyhow::Result<()>; // alias for Result<(), anyhow::Error>

pub(crate) fn create_default_os(
    _chain: &Mock,
    factory: &OSFactory<Mock>,
) -> anyhow::Result<OS<Mock>> {
    let os = factory.create_default_os(GovernanceDetails::Monarchy {
        monarch: Addr::unchecked(ROOT_USER).to_string(),
    })?;
    Ok(os)
}

/// Instantiates the staking api and registers it with the version control
#[allow(dead_code)]
pub(crate) fn init_exchange(
    chain: &Mock,
    deployment: &Deployment<Mock>,
    version: Option<String>,
) -> anyhow::Result<DexApi<Mock>> {
    let mut exchange = DexApi::new(TENDERMINT_STAKING, chain.clone());
    exchange.as_instance_mut().set_mock(Box::new(
        cw_multi_test::ContractWrapper::new_with_empty(
            ::dex::contract::execute,
            ::dex::contract::instantiate,
            ::dex::contract::query,
        ),
    ));
    exchange.upload()?;
    exchange.instantiate(
        &InstantiateMsg {
            app: Empty {},
            base: abstract_os::api::BaseInstantiateMsg {
                ans_host_address: deployment.ans_host.addr_str()?,
                version_control_address: deployment.version_control.addr_str()?,
            },
        },
        None,
        None,
    )?;

    let version: semver::Version = version.map(|s| s.parse().unwrap()).unwrap_or(deployment.version.clone());

    deployment
        .version_control
        .register_apis(vec![exchange.as_instance()], &version)?;
    Ok(exchange)
}
