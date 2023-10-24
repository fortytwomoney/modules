use abstract_core::ans_host::QueryMsgFns;
use abstract_core::module_factory::ModuleInstallConfig;
use abstract_core::objects::{AccountId, AssetEntry};
use autocompounder::interface::AutocompounderApp;
use autocompounder::kujira_tx::TOKEN_FACTORY_CREATION_FEE;
use autocompounder::msg::AutocompounderExecuteMsgFns;
use cw_orch::daemon::DaemonBuilder;
use cw_orch::deploy::Deploy;
use cw_orch::environment::{CwEnv, TxResponse};
use cw_orch::prelude::queriers::{Bank, DaemonQuerier};
use cw_orch::prelude::*;
use std::env;
use std::sync::Arc;

use abstract_core::{
    account_factory, adapter, app,
    objects::module::ModuleInfo,
    manager::ExecuteMsg,
    objects::{gov_type::GovernanceDetails, module::ModuleVersion},
    registry::{ANS_HOST, MANAGER, PROXY},
    ABSTRACT_EVENT_TYPE,
};
use abstract_cw_staking::CW_STAKING;
use abstract_interface::{
    Abstract, AbstractAccount, AccountDetails, AccountFactory, Manager, ManagerExecFns,
    ManagerQueryFns, Proxy,

};

use clap::Parser;
use cosmwasm_std::{coin, to_binary, Addr, Decimal, Empty};
use cw_orch::daemon::networks::parse_network;

use autocompounder::msg::{AutocompounderInstantiateMsg, BondingPeriodSelector, AUTOCOMPOUNDER};
use log::info;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn description(asset_string: String) -> String {
    return format!(
        "Within the vault, users {} LP tokens are strategically placed into an Astroport farm, generating the platform governance token as rewards. These earned tokens are intelligently exchanged to acquire additional underlying assets, further boosting the volume of the same liquidity tokens. The newly acquired axlUSDC/ASTRO LP tokens are promptly integrated back into the farm, primed for upcoming earning events. The transaction costs associated with these processes are distributed among the users of the vault, creating a collective and efficient approach.",
        asset_string
    );
}

fn create_vault_account<Chain: CwEnv>(
    factory: &AccountFactory<Chain>,
    chain: Chain,
    governance_details: GovernanceDetails<String>,
    assets: Vec<String>,
    modules: Vec<ModuleInstallConfig>,
    coins: Option<&[Coin]>,
) -> Result<AbstractAccount<Chain>, abstract_interface::AbstractInterfaceError>
where
    TxResponse<Chain>: IndexResponse,
{
    let result = factory.create_new_account(
        AccountDetails {
            // &account_factory::ExecuteMsg::CreateAccount {
            base_asset: None,
            namespace: Some("4t2".to_string()),
            install_modules: modules,
            description: None,
            link: None,
            name: format!("4t2 Vault ({})", assets.join("|").replace('>', ":")),
        },
        governance_details,
        coins,
    )?;

    Ok(result)
}

fn init_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (main_account_id, dex, base_pair_asset, cw20_code_id) = match args.network_id.as_str() {
        "uni-6" => (None, "wyndex", "juno>junox", Some(4012)),
        "juno-1" => (None, "wyndex", "juno>juno", Some(1)),
        "pion-1" => (None, "astroport", "neutron>astro", Some(188)),
        "neutron-1" => (None, "astroport", "neutron>astro", Some(180)),
        "pisco-1" => (None, "astroport", "terra2>luna", Some(83)),
        "phoenix-1" => (None, "astroport", "terra2>luna", Some(69)),
        "osmo-test-5" => (Some(2), "osmosis5", "osmosis5>osmo", Some(1)),
        "harpoon-4" => (Some(2), "kujira", "kujira>kuji", None),
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

    let abstr = Abstract::load_from(chain.clone())?;
    let main_account = if let Some(account_id) = main_account_id {
        AbstractAccount::new(&abstr, Some(AccountId::local(account_id)))
    } else {
        panic!("Not implemented yet");
        abstr.account_factory.create_new_account(
            AccountDetails {
                name: "fortytwo manager".to_string(),
                description: Some("manager of 4t2 smartcontracts".to_string()),
                link: None,
                namespace: Some("4t2".to_string()),
                base_asset: None,
                install_modules: vec![],
            },
            GovernanceDetails::Monarchy {
                monarch: sender.to_string(),
            },
            None,
        )?
    };



    info!(
        "Created account: {:?}",
        main_account
            .manager
            .sub_account_ids(None, None)?
            .sub_accounts
            .last()
    );

    let asset_list = abstr.ans_host.asset_info_list(None, None, None)?;


    info!("Asset list: {:?}", asset_list);

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Network to deploy on
    #[arg(short, long)]
    network_id: String,

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
