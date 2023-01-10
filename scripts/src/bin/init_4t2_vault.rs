use std::env;
use std::sync::Arc;
use abstract_boot::{OS, OSFactory, VersionControl};
use abstract_os::{api, app, OS_FACTORY, VERSION_CONTROL};
use abstract_os::manager::QueryMsgFns;
use abstract_os::objects::AssetEntry;
use abstract_os::objects::gov_type::GovernanceDetails;
use abstract_os::objects::module::ModuleVersion;
use abstract_os::os_factory::ExecuteMsgFns;

use boot_core::networks::NetworkInfo;
use boot_core::prelude::*;
use boot_core::{networks, DaemonOptionsBuilder, Contract};
use cosmwasm_std::Addr;
use semver::Version;
use forty_two::autocompounder;
use forty_two::autocompounder::{AUTOCOMPOUNDER, AutocompounderInstantiateMsg};
use forty_two::cw_staking::CW_STAKING;
use forty_two_boot::cw_staking::CwStakingApi;

const NETWORK: NetworkInfo = networks::UNI_5;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test OS that uses that new app

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn deploy_api() -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let daemon_options = DaemonOptionsBuilder::default().network(NETWORK).build()?;

    // Setup the environment
    let (_sender, chain) = instantiate_daemon_env(&rt, daemon_options)?;

    // // Load Abstract Version Control
    // let _version_control_address: String =
    //     env::var("VERSION_CONTROL_ADDRESS").expect("VERSION_CONTROL_ADDRESS must be set");
    // let _version_control_address: String =
    //     env::var("VERSION_CONTROL_ADDRESS").expect("VERSION_CONTROL_ADDRESS must be set");

    let version_control = VersionControl::load(
        chain.clone(),
        &Addr::unchecked("juno1q8tuzav8y6aawhc4sddqnwj6q4gdvn7lyk3m9ks4uw69xp37j83ql3ck2q"),
    );

    let mut os_factory = OSFactory::new(
        OS_FACTORY,
        chain.clone(),
    );

    let abstract_version = std::env::var("ABSTRACT_VERSION").expect("Missing ABSTRACT_VERSION");

    let abstract_version = ModuleVersion::from(abstract_version.to_string());
    os_factory.set_address(&version_control.get_api_addr(OS_FACTORY, abstract_version.clone())?);

    //
    // let os = os_factory.create_default_os(GovernanceDetails::Monarchy {
    //     monarch: _sender.to_string(),
    // })?;

    let cw_staking = CwStakingApi::load(chain.clone(), &Addr::unchecked("juno1vgrxcupau9zr3z85rar7aq7v28v47s4tgdjm4xasxx96ap8wdzssfwfx27"));

    let query_res = forty_two::cw_staking::CwStakingQueryMsgFns::info(&cw_staking, "junoswap", AssetEntry::new("junoswap/crab,junox"))?;
    // panic!("{?:}", query_res);

    let os2 = OS::new(&chain.clone(), None);

    if !os2.manager.module_infos(None, None)?.module_infos.iter().any(|module_info| module_info.id == CW_STAKING) {
        os2.manager.install_module_version(CW_STAKING, ModuleVersion::from(MODULE_VERSION),Some(&api::InstantiateMsg {
            base: api::BaseInstantiateMsg {
                ans_host_address: "juno1qyetxuhvmpgan5qyjq3julmzz9g3rhn3jfp2jlgy29ftjknv0c6s0xywpp".to_string(),
                version_control_address: version_control.addr_str()?
            },
            app: {},
        }))?;
    }

    // let os2 = Os::new

    os2.manager.install_module(AUTOCOMPOUNDER, Some(&app::InstantiateMsg {
        base: app::BaseInstantiateMsg {
            ans_host_address: "juno1qyetxuhvmpgan5qyjq3julmzz9g3rhn3jfp2jlgy29ftjknv0c6s0xywpp".to_string(),
            // ans_host_address: version_control.get_api_addr(OS_FACTORY, abstract_version)?.to_string()
        },
        app: AutocompounderInstantiateMsg
        {
            performance_fees: 100u128.into(),
            deposit_fees: 100u128.into(),
            withdrawal_fees: 100u128.into(),
            /// address that recieves the fee commissions
            commission_addr: _sender.to_string(),
            /// cw20 code id
            code_id: 4012,
            /// Name of the target dex
            dex: "junoswap".into(),
            /// Assets in the pool
            pool_assets: vec!["crab".into(), "junox".into()],
            // pool_assets: vec!["junox".into(), "crab".into()],
        },
    }))?;


    // let app_config: ConfigResponse = app.query_app(AutocompounderQueryMsg::Config {})?;

    // TODO: Attach to an OS

    Ok(())
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    if let Err(ref err) = deploy_api() {
        log::error!("{}", err);
        err.chain()
            .skip(1)
            .for_each(|cause| log::error!("because: {}", cause));

        // The backtrace is not always generated. Try to run this example
        // with `$env:RUST_BACKTRACE=1`.
        //    if let Some(backtrace) = e.backtrace() {
        //        log::debug!("backtrace: {:?}", backtrace);
        //    }

        ::std::process::exit(1);
    }
}
