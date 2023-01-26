use abstract_boot::{Manager, OSFactory, Proxy, VersionControl, OS};
use abstract_os::manager::QueryMsgFns;
use abstract_os::objects::gov_type::GovernanceDetails;
use abstract_os::objects::module::ModuleVersion;
use abstract_os::{app, EXCHANGE, OS_FACTORY};
use abstract_os::{os_factory, MANAGER, PROXY};
use boot_core::networks::NetworkInfo;
use boot_core::prelude::*;
use boot_core::state::StateInterface;
use boot_core::{networks, DaemonOptionsBuilder};
use cosmwasm_std::{Addr, Decimal, Empty};
use forty_two::autocompounder::{
    AutocompounderInstantiateMsg, BondingPeriodSelector, AUTOCOMPOUNDER,
};
use forty_two::cw_staking::CW_STAKING;
use std::env;
use std::sync::Arc;

const NETWORK: NetworkInfo = networks::UNI_5;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test OS that uses that new app

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

// TODO: abstract boot
fn is_module_installed<Chain: BootEnvironment>(
    os: &OS<Chain>,
    module_id: &str,
) -> anyhow::Result<bool> {
    let module_infos = os.manager.module_infos(None, None)?.module_infos;
    Ok(module_infos
        .iter()
        .any(|module_info| module_info.id == module_id))
}

fn create_vault<Chain: BootEnvironment>(
    factory: &OSFactory<Chain>,
    chain: Chain,
    governance_details: GovernanceDetails,
    assets: Vec<String>,
) -> Result<OS<Chain>, BootError> {
    let result = factory.execute(
        &os_factory::ExecuteMsg::CreateOs {
            governance: governance_details,
            description: None,
            link: None,
            name: format!("4t2 Vault ({})", assets.join("|")),
        },
        None,
    )?;

    let manager_address = &result.event_attr_value("wasm", "manager_address")?;
    chain
        .state()
        .set_address(MANAGER, &Addr::unchecked(manager_address));
    let proxy_address = &result.event_attr_value("wasm", "proxy_address")?;
    chain
        .state()
        .set_address(PROXY, &Addr::unchecked(proxy_address));
    Ok(OS {
        manager: Manager::new(MANAGER, chain.clone()),
        proxy: Proxy::new(PROXY, chain),
    })
}

fn deploy_api(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let daemon_options = DaemonOptionsBuilder::default().network(NETWORK).build()?;

    // Setup the environment
    let (sender, chain) = instantiate_daemon_env(&rt, daemon_options)?;

    // // Load Abstract Version Control
    // let _version_control_address: String =
    //     env::var("VERSION_CONTROL_ADDRESS").expect("VERSION_CONTROL_ADDRESS must be set");
    // let _version_control_address: String =
    //     env::var("VERSION_CONTROL_ADDRESS").expect("VERSION_CONTROL_ADDRESS must be set");

    let version_control = VersionControl::load(
        chain.clone(),
        &Addr::unchecked("juno1q8tuzav8y6aawhc4sddqnwj6q4gdvn7lyk3m9ks4uw69xp37j83ql3ck2q"),
    );

    let os_factory = OSFactory::new(OS_FACTORY, chain.clone());

    let abstract_version = std::env::var("ABSTRACT_VERSION").expect("Missing ABSTRACT_VERSION");

    let abstract_version = ModuleVersion::from(abstract_version);
    os_factory.set_address(&version_control.get_api_addr(OS_FACTORY, abstract_version)?);

    let mut assets = vec![args.paired_asset, "junox".to_string()];
    assets.sort();

    let os = if let Some(os_id) = args.os_id {
        OS::new(chain, Some(os_id))
    } else {
        create_vault(
            &os_factory,
            chain,
            GovernanceDetails::Monarchy {
                monarch: sender.to_string(),
            },
            assets.clone(),
        )?
    };

    // let _cw_staking = CwStakingApi::load(chain.clone(), &Addr::unchecked("juno1vgrxcupau9zr3z85rar7aq7v28v47s4tgdjm4xasxx96ap8wdzssfwfx27"));

    // let query_res = forty_two::cw_staking::CwStakingQueryMsgFns::info(&cw_staking, "junoswap", AssetEntry::new("junoswap/crab,junox"))?;
    // panic!("{?:}", query_res);

    // First uninstall autocompounder if found
    if is_module_installed(&os, AUTOCOMPOUNDER)? {
        os.manager.uninstall_module(AUTOCOMPOUNDER)?;
    }

    // Uninstall cw_staking if found
    if is_module_installed(&os, CW_STAKING)? {
        os.manager.uninstall_module(CW_STAKING)?;
    }

    // Install both modules
    let new_module_version = ModuleVersion::from(MODULE_VERSION);

    os.manager
        .install_module_version(CW_STAKING, new_module_version.clone(), &Empty {})?;

    // Install abstract dex
    if !is_module_installed(&os, EXCHANGE)? {
        os.manager.install_module(EXCHANGE, &Empty {})?;
    }

    os.manager.install_module_version(
        AUTOCOMPOUNDER,
        new_module_version,
        &app::InstantiateMsg {
            base: app::BaseInstantiateMsg {
                ans_host_address: "juno1qyetxuhvmpgan5qyjq3julmzz9g3rhn3jfp2jlgy29ftjknv0c6s0xywpp"
                    .to_string(),
                // ans_host_address: version_control.get_api_addr(ANS_HOST, abstract_version)?.to_string()
            },
            app: AutocompounderInstantiateMsg {
                performance_fees: Decimal::new(100u128.into()),
                deposit_fees: Decimal::new(100u128.into()),
                withdrawal_fees: Decimal::new(100u128.into()),
                /// address that recieves the fee commissions
                commission_addr: sender.to_string(),
                /// cw20 code id
                code_id: 4012,
                /// Name of the target dex
                dex: "junoswap".into(),
                fee_asset: "junox".into(),
                /// Assets in the pool
                pool_assets: assets.into_iter().map(Into::into).collect(),
                preferred_bonding_period: BondingPeriodSelector::Shortest,
            },
        },
    )?;

    Ok(())
}

use clap::Parser;
#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Whether the OS is new or not (TODO: just take in OSId)
    #[arg(short, long)]
    os_id: Option<u32>,
    /// Paired asset in the pool
    #[arg(short, long)]
    paired_asset: String,
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    if let Err(ref err) = deploy_api(args) {
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
