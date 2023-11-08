use abstract_core::module_factory::ModuleInstallConfig;
use abstract_core::objects::namespace::Namespace;
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
    manager::ExecuteMsg,
    objects::module::ModuleInfo,
    objects::{gov_type::GovernanceDetails, module::ModuleVersion},
    registry::{ANS_HOST, MANAGER, PROXY},
    ABSTRACT_EVENT_TYPE,
};
use abstract_cw_staking::CW_STAKING;
use abstract_dex_adapter::EXCHANGE;
use abstract_interface::{
    Abstract, AbstractAccount, AccountDetails, AccountFactory, Manager, ManagerExecFns,
    ManagerQueryFns, Proxy, VCExecFns,
};

use clap::Parser;
use cosmwasm_std::{coin, to_binary, Addr, Decimal, Empty};
use cw_orch::daemon::networks::parse_network;

use autocompounder::interface::Vault;
use autocompounder::msg::{
    AutocompounderInstantiateMsg, AutocompounderQueryMsgFns, BondingPeriodSelector, AUTOCOMPOUNDER,
};
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

    let (main_account_id, dex, base_pair_asset, cw20_code_id,
    token_creation_fee) = match args.network_id.as_str() {
        "uni-6" => (None, "wyndex", "juno>junox", Some(4012), None),
        "juno-1" => (None, "wyndex", "juno>juno", Some(1), None),
        "pion-1" => (None, "astroport", "neutron>astro", Some(188), None),
        "neutron-1" => (None, "astroport", "neutron>astro", Some(180), None),
        "pisco-1" => (None, "astroport", "terra2>luna", Some(83), None),
        "phoenix-1" => (None, "astroport", "terra2>luna", Some(69), None),
        "osmo-test-5" => (Some(2), "osmosis5", "osmosis5>osmo", None, None),
        "harpoon-4" => (Some(2), "kujira", "kujira>kuji", None, Some(TOKEN_FACTORY_CREATION_FEE)),
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

    let instantiation_funds: Option<Vec<Coin>> = if let Some(creation_fee) = token_creation_fee {
        let bank = Bank::new(chain.channel());
        let balance: u128 = rt
            .block_on(bank.balance(&sender, Some("ukuji".to_string())))
            .unwrap()[0]
            .amount
            .parse()?;
        if balance < creation_fee {
            panic!("Not enough ukuji to pay for token factory creation fee");
        }
        Some(vec![coin(creation_fee, "ukuji")])
    } else {
        None
    };

    // let update = abstr.version_control.update_module_configuration(
    //     AUTOCOMPOUNDER.to_string(),
    //     Namespace::new("4t2")?,
    //     abstract_core::version_control::UpdateModule::Versioned {
    //         version: MODULE_VERSION.to_string(),
    //         metadata: None,
    //         monetization: None,
    //         instantiation_funds: instantiation_funds.clone(),
    //     },
    // )?;

    // info!("Updated module: {:?}", update);

    let mut pair_assets = vec![args.paired_asset, args.other_asset];
    pair_assets.sort();

    // let new_module_version =
    // ModuleVersion::Version(args.ac_version.unwrap_or(MODULE_VERSION.to_string()));

    let autocompounder_instantiate_msg = &app::InstantiateMsg {
        base: app::BaseInstantiateMsg {
            ans_host_address: abstr
                .version_control
                .module(ModuleInfo::from_id_latest(ANS_HOST)?)?
                .reference
                .unwrap_addr()?
                .to_string(),
            version_control_address: abstr.version_control.address()?.to_string(),
        },
        module: AutocompounderInstantiateMsg {
            performance_fees: Decimal::new(100u128.into()),
            deposit_fees: Decimal::new(0u128.into()),
            withdrawal_fees: Decimal::new(0u128.into()),
            /// address that recieves the fee commissions
            commission_addr: sender.to_string(),
            /// cw20 code id
            code_id: cw20_code_id,
            /// Name of the target dex
            dex: dex.into(),
            /// Assets in the pool
            pool_assets: pair_assets.clone().into_iter().map(Into::into).collect(),
            preferred_bonding_period: BondingPeriodSelector::Shortest,
            max_swap_spread: Some(Decimal::percent(10)),
        },
    };

    let manager_create_sub_account_msg = ExecuteMsg::CreateSubAccount {
        base_asset: None,
        namespace: None,
        description: None,
        link: None,
        name: format!("4t2 Vault ({})", pair_assets.join("|").replace('>', ":")),
        install_modules: vec![
            // installs both abstract dex and staking in the instantiation of the account
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(EXCHANGE)?, None),
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(CW_STAKING)?, None),
            // ModuleInstallConfig::new(ModuleInfo::from_id_latest(AUTOCOMPOUNDER)?, autocompounder_instantiate_msg)
        ],
    };

    let result = main_account.manager.execute(
        &manager_create_sub_account_msg,
        instantiation_funds.as_deref(),
    )?;
    info!(
        "Instantiated AC addr: {}",
        result.instantiated_contract_address()?.to_string()
    );

    let new_vault_account_id = main_account
        .manager
        .sub_account_ids(None, None)?
        .sub_accounts
        .last()
        .unwrap()
        .to_owned();

    let new_account =
        AbstractAccount::new(&abstr, Some(AccountId::local(new_vault_account_id.clone())));
    new_account.install_module(
        format!("4t2:{}",AUTOCOMPOUNDER).as_str(),
        &autocompounder_instantiate_msg,
        instantiation_funds.as_deref(),
    )?;

    let new_vault = Vault::new(&abstr, Some(AccountId::local(new_vault_account_id)))?;
    let installed_modules = new_account.manager.module_infos(None, None)?;
    let vault_config = new_vault.autocompounder.config()?;

    info!(
        "
    Vault created with account id: {} 
    modules: {:?}\n
    config: §{:?}\n
    ",
        new_vault_account_id, installed_modules, vault_config
    );

    // let result = abstr.account_factory.create_new_account(
    //     AccountDetails {
    //     // &account_factory::ExecuteMsg::CreateAccount {
    //     GovernanceDetails::SubAccount { manager: main_account.manager.addr_str()?, proxy: main_account.proxy.addr_str()? },
    //     instantiation_funds.as_deref(),
    // )?;

    info!(
        "Created account: {:?}",
        main_account
            .manager
            .sub_account_ids(None, None)?
            .sub_accounts
            .last()
    );

    Ok(())
}

#[derive(Parser, Default, Debug)]
#[command(author, version, about, long_about = None)]
struct Arguments {
    /// Paired asset in the pool
    #[arg(short, long)]
    paired_asset: String,
    /// Asset1 in the pool
    #[arg(short, long)]
    other_asset: String,
    /// Network to deploy on
    #[arg(short, long)]
    network_id: String,
    /// Autocompounder version to deploy. None means latest
    #[arg(long)]
    ac_version: Option<String>,
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
