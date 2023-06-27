use autocompounder::interface::{get_module_address, is_module_installed};
use cw_orch::daemon::DaemonBuilder;
use cw_orch::environment::{CwEnv, TxResponse};
use cw_orch::prelude::*;
use std::env;
use std::sync::Arc;

use abstract_core::{
    account_factory, adapter, app,
    manager::ExecuteMsgFns,
    objects::module::ModuleInfo,
    objects::{gov_type::GovernanceDetails, module::ModuleVersion},
    registry::{ANS_HOST, MANAGER, PROXY},
    ABSTRACT_EVENT_NAME,
};
use abstract_cw_staking::CW_STAKING;
use abstract_interface::{Abstract, AbstractAccount, AccountFactory, Manager, Proxy};

use clap::Parser;
use cosmwasm_std::{Addr, Decimal, Empty};
use cw_orch::daemon::networks::parse_network;

use autocompounder::msg::{AutocompounderInstantiateMsg, BondingPeriodSelector, AUTOCOMPOUNDER};
use log::info;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn create_vault_account<Chain: CwEnv>(
    factory: &AccountFactory<Chain>,
    chain: Chain,
    governance_details: GovernanceDetails<String>,
    assets: Vec<String>,
) -> Result<AbstractAccount<Chain>, CwOrchError>
where
    TxResponse<Chain>: IndexResponse,
{
    let result = factory.execute(
        &account_factory::ExecuteMsg::CreateAccount {
            governance: governance_details,
            description: None,
            link: None,
            name: format!("4t2 Vault ({})", assets.join("|")),
        },
        None,
    )?;

    let manager_address = &result.event_attr_value(ABSTRACT_EVENT_NAME, "manager_address")?;
    chain
        .state()
        .set_address(MANAGER, &Addr::unchecked(manager_address));
    let proxy_address = &result.event_attr_value(ABSTRACT_EVENT_NAME, "proxy_address")?;
    chain
        .state()
        .set_address(PROXY, &Addr::unchecked(proxy_address));
    Ok(AbstractAccount {
        manager: Manager::new(MANAGER, chain.clone()),
        proxy: Proxy::new(PROXY, chain),
    })
}

fn init_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (dex, base_pair_asset, cw20_code_id) = match args.network_id.as_str() {
        // "uni-6" => ("wyndex", "juno>junox", 4012),
        "juno-1" => ("wyndex", "juno>juno", 1),
        "pion-1" => ("astroport", "neutron>astro", 188),
        // "pisco-1" => ("astroport", "terra2>luna", 83),
        _ => panic!("Unknown network id: {}", args.network_id),
    };

    info!("Using dex: {} and base: {}", dex, base_pair_asset);

    // Setup the environment
    let network = parse_network(&args.network_id);

    // TODO: make grpc url dynamic by removing this line once cw-orch gets updated
    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;
    let sender = chain.sender();

    let abstr = Abstract::new(chain.clone());

    let mut pair_assets = vec![args.paired_asset, args.other_asset];
    pair_assets.sort();

    let account = if let Some(account_id) = args.account_id {
        AbstractAccount::new(chain, Some(account_id))
    } else {
        create_vault_account(
            &abstr.account_factory,
            chain,
            GovernanceDetails::Monarchy {
                monarch: sender.to_string(),
            },
            pair_assets.clone(),
        )?
    };

    // let query_res = forty_two ::abstract_cw_staking::CwStakingQueryMsgFns::info(&cw_staking, "junoswap", AssetEntry::new("junoswap/crab,junox"))?;
    // panic!("{?:}", query_res);

    // Install abstract dex
    if !is_module_installed(&account, "abstract:dex")? {
        account
            .manager
            .install_module("abstract:dex", &Empty {}, None)?;
    }

    // install the staking module
    if !is_module_installed(&account, CW_STAKING)? {
        account
            .manager
            .install_module(CW_STAKING, &Empty {}, None)?;
    }
    // First uninstall autocompounder if found
    if is_module_installed(&account, AUTOCOMPOUNDER)? {
        account
            .manager
            .uninstall_module(AUTOCOMPOUNDER.to_string())?;
    }

    // Install both modules
    let new_module_version = ModuleVersion::from(MODULE_VERSION);

    // account.manager.install_module(CW_STAKING, &Empty {})?;

    account.manager.install_module_version(
        AUTOCOMPOUNDER,
        new_module_version,
        &app::InstantiateMsg {
            base: app::BaseInstantiateMsg {
                ans_host_address: abstr
                    .version_control
                    .module(ModuleInfo::from_id_latest(ANS_HOST)?)?
                    .reference
                    .unwrap_addr()?
                    .to_string(),
            },
            module: AutocompounderInstantiateMsg {
                performance_fees: Decimal::new(100u128.into()),
                deposit_fees: Decimal::new(100u128.into()),
                withdrawal_fees: Decimal::new(100u128.into()),
                /// address that recieves the fee commissions
                commission_addr: sender.to_string(),
                /// cw20 code id
                code_id: cw20_code_id,
                /// Name of the target dex
                dex: dex.into(),
                /// Assets in the pool
                pool_assets: pair_assets.into_iter().map(Into::into).collect(),
                preferred_bonding_period: BondingPeriodSelector::Shortest,
                max_swap_spread: Some(Decimal::percent(10)),
            },
        },
        None,
    )?;

    // Register the autocompounder as a trader on the cw-staking and the dex
    let autocompounder_address = get_module_address(&account, AUTOCOMPOUNDER)?;

    account.manager.execute_on_module(
        CW_STAKING,
        adapter::ExecuteMsg::<Empty, Empty>::Base(
            adapter::BaseExecuteMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_address.to_string()],
                to_remove: vec![],
            },
        ),
    )?;

    account.manager.execute_on_module(
        "abstract:dex",
        adapter::ExecuteMsg::<Empty, Empty>::Base(
            adapter::BaseExecuteMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_address.to_string()],
                to_remove: vec![],
            },
        ),
    )?;

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Optionally provide an Account Id to turn into a vault
    #[arg(short, long)]
    account_id: Option<u32>,
    /// Paired asset in the pool
    #[arg(short, long)]
    paired_asset: String,
    /// Asset1 in the pool
    #[arg(short, long)]
    other_asset: String,
    /// Network to deploy on
    #[arg(short, long)]
    network_id: String,
    // #[arg(short, long)]
    // dex: String,
}

fn main() {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    let args = Arguments::parse();

    if let Err(ref err) = init_vault(args) {
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
