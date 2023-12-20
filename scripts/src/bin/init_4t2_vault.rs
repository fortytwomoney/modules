use abstract_core::module_factory::ModuleInstallConfig;
use abstract_core::objects::{AccountId, AssetEntry, DexAssetPairing};
use autocompounder::kujira_tx::TOKEN_FACTORY_CREATION_FEE;
use cw_orch::daemon::networks::osmosis::OSMO_NETWORK;
use cw_orch::daemon::{ChainInfo, ChainKind, DaemonBuilder};
use cw_orch::deploy::Deploy;
use cw_orch::prelude::queriers::{Bank, DaemonQuerier};
use cw_orch::prelude::*;
use std::env;

use abstract_core::{app, manager::ExecuteMsg, objects::gov_type::GovernanceDetails, objects::module::ModuleInfo, proxy, PROXY, registry::ANS_HOST};
use abstract_cw_staking::CW_STAKING;
use abstract_dex_adapter::EXCHANGE;
use abstract_interface::{Abstract, AbstractAccount, AccountDetails, ManagerQueryFns};
use cw_utils::Duration;
use std::sync::Arc;
use abstract_core::proxy::{AssetsConfigResponse, QueryMsgFns};
use abstract_core::manager::ExecuteMsgFns;
use abstract_core::objects::price_source::UncheckedPriceSource;

use clap::Parser;
use cosmwasm_std::{coin, Addr, Decimal, to_binary};
use cw_orch::daemon::networks::parse_network;

use autocompounder::interface::{AutocompounderApp, Vault};
use autocompounder::msg::{AutocompounderInstantiateMsg, AutocompounderQueryMsgFns, BondingData, AUTOCOMPOUNDER_ID};
use log::info;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

const _MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn description(asset_string: String) -> String {
    format!(
        "Within the vault, users {} LP tokens are strategically placed into an Astroport farm, generating the platform governance token as rewards. These earned tokens are intelligently exchanged to acquire additional underlying assets, further boosting the volume of the same liquidity tokens. The newly acquired axlUSDC/ASTRO LP tokens are promptly integrated back into the farm, primed for upcoming earning events. The transaction costs associated with these processes are distributed among the users of the vault, creating a collective and efficient approach.",
        asset_string
    )
}

fn init_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (main_account_id, dex, base_pair_asset, cw20_code_id, token_creation_fee) =
        match args.network_id.as_str() {
            "uni-6" => (None, "wyndex", "juno>junox", Some(4012), None),
            "juno-1" => (None, "wyndex", "juno>juno", Some(1), None),
            "pion-1" => (None, "astroport", "neutron>astro", Some(188), None),
            "neutron-1" => (None, "astroport", "neutron>astro", Some(180), None),
            "pisco-1" => (None, "astroport", "terra2>luna", Some(83), None),
            "phoenix-1" => (None, "astroport", "terra2>luna", Some(69), None),
            "osmo-test-5" => (Some(2), "osmosis", "osmosis>osmo", None, None),
            "osmosis-1" => (Some(5), "osmosis", "osmosis>osmo", None, None),
            "harpoon-4" => (
                Some(2),
                "kujira",
                "kujira>kuji",
                None,
                Some(TOKEN_FACTORY_CREATION_FEE),
            ),
            _ => panic!("Unknown network id: {}", args.network_id),
        };

    info!("Using dex: {} and base: {}", dex, base_pair_asset);

    // Setup the environment
    pub const OSMOSIS_1: ChainInfo = ChainInfo {
        kind: ChainKind::Mainnet,
        chain_id: "osmosis-1",
        gas_denom: "uosmo",
        gas_price: 0.025,
        grpc_urls: &["http://grpc.osmosis.zone:9090"],
        network_info: OSMO_NETWORK,
        lcd_url: None,
        fcd_url: None,
    };

    let network: ChainInfo = if &args.network_id == "osmosis-1" {
        OSMOSIS_1
    } else {
        parse_network(&args.network_id)
    };

    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;
    let sender = chain.sender();

    let abstr = Abstract::load_from(chain.clone())?;
    let parent_account = get_parent_account(main_account_id, &abstr, &sender)?;

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

    // info!("Updated module: {:?}", update);

    let mut vault_pair_assets = vec![args.paired_asset.clone(), args.other_asset.clone()];
    vault_pair_assets.sort();

    // let new_module_version =
    // ModuleVersion::Version(args.ac_version.unwrap_or(MODULE_VERSION.to_string()));
    let bonding_data = match (args.unbonding_period_blocks, args.unbonding_period_time) {
        (Some(_), Some(_)) => panic!("cant set both unbonding period as blocks and as time"),
        (Some(blocks), None) => Some(BondingData {
            unbonding_period: Duration::Height(blocks),
            max_claims_per_address: args.max_claims_per_address,
        }),
        (None, Some(seconds)) => Some(BondingData {
            unbonding_period: Duration::Time(seconds),
            max_claims_per_address: args.max_claims_per_address,
        }),
        (None, None) => None,
    };

    let autocompounder_mod_init_msg = AutocompounderInstantiateMsg {
        performance_fees: Decimal::new(100u128.into()),
        deposit_fees: Decimal::new(0u128.into()),
        withdrawal_fees: Decimal::new(0u128.into()),
        // address that receives the fee commissions
        commission_addr: sender.to_string(),
        // cw20 code id
        code_id: cw20_code_id,
        // Name of the target dex
        dex: dex.into(),
        // Assets in the pool
        pool_assets: vault_pair_assets.clone().into_iter().map(Into::into).collect(),
        bonding_data,
        max_swap_spread: Some(Decimal::percent(10)),
    };

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
        module: autocompounder_mod_init_msg,
    };

    let result = parent_account.manager.create_sub_account(
        vec![
            // installs both abstract dex and staking in the instantiation of the account
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(EXCHANGE)?, None),
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(CW_STAKING)?, None),
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(AUTOCOMPOUNDER_ID)?, Some(to_binary(autocompounder_instantiate_msg)?))
        ],
        format!("4t2 Vault ({})", vault_pair_assets.join("|").replace('>', ":")),
        Some(base_pair_asset.into()),
        Some(description(vault_pair_assets.join("|").replace('>', ":"))),
        None,
        None,
    )?;

    info!(
        "Instantiated AC addr: {}",
        result.instantiated_contract_address()?.to_string()
    );

    let new_vault_account_id = parent_account
        .manager
        .sub_account_ids(None, None)?
        .sub_accounts
        .last()
        .unwrap()
        .to_owned();

    let new_vault_account = AbstractAccount::new(&abstr, Some(AccountId::local(new_vault_account_id)));
    println!("New vault account id: {:?}", new_vault_account.id()?);

    // Osmosis does not support value calculation via pools
    if dex != "osmosis" {
        let base_asset: AssetEntry = base_pair_asset.into();
        let paired_asset: AssetEntry = if args.paired_asset == base_pair_asset.to_string() { args.other_asset.into() } else { args.paired_asset.into() };
        register_assets(&new_vault_account, base_asset, paired_asset, dex, vault_pair_assets)?;
    }

    let new_vault = Vault::new(&abstr, AccountId::local(new_vault_account_id))?;
    let installed_modules = new_vault_account.manager.module_infos(None, None)?;
    let vault_config = new_vault.autocompounder.config()?;

    info!(
        "
    Vault created with account id: {} 
    modules: {:?}\n
    config: §{:?}\n
    ",
        new_vault_account_id, installed_modules, vault_config
    );

    Ok(())
}

/// Register the assets on the account for value calculation
fn register_assets(vault_account: &AbstractAccount<Daemon>, base_pair_asset: AssetEntry, paired_asset: AssetEntry, dex: &str, vault_pair_assets: Vec<String>) -> Result<(), anyhow::Error> {
    let AssetsConfigResponse {
        assets: actual_registered_assets
    } = vault_account.proxy.assets_config(None, None)?;

    let actual_registered_entries = actual_registered_assets
        .into_iter()
        .map(|(asset, _)| asset.clone())
        .collect::<Vec<_>>();


    // Register the assets on the vault
    let expected_registered_assets = vec![
        (base_pair_asset, UncheckedPriceSource::None),
        (paired_asset, UncheckedPriceSource::Pair(DexAssetPairing::new(
            vault_pair_assets[0].clone().into(),
            vault_pair_assets[1].clone().into(),
            dex
        )))
    ];

    let assets_to_register = expected_registered_assets
        .into_iter()
        .filter(|(asset, _)| !actual_registered_entries.contains(asset))
        .map(|(asset, price_source)| (asset.clone(), price_source.clone()))
        .collect::<Vec<_>>();

    println!("Registering assets: {:?}", assets_to_register);

    vault_account.manager.execute_on_module(PROXY, proxy::ExecuteMsg::UpdateAssets {
        to_add: assets_to_register,
        to_remove: vec![],
    })?;
    Ok(())
}

/// Retrieve the account that will have all the 4t2 vaults as sub-accounts
fn get_parent_account(
    main_account_id: Option<u32>,
    abstr: &Abstract<Daemon>,
    sender: &Addr,
) -> Result<AbstractAccount<Daemon>, anyhow::Error> {
    let main_account = if let Some(account_id) = main_account_id {
        AbstractAccount::new(abstr, Some(AccountId::local(account_id)))
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
    Ok(main_account)
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
    /// Optional unbonding period in seconds
    #[arg(long)]
    unbonding_period_time: Option<u64>,
    /// Optional unbonding period in blocks
    #[arg(long)]
    unbonding_period_blocks: Option<u64>,
    /// Optional max claims per address
    #[arg(long)]
    max_claims_per_address: Option<u32>,
    /// Force creating a new vault instead of loading an existing one
    #[arg(long)]
    force_new: bool,
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
