use abstract_client::{AbstractClient, Account, Namespace};
use abstract_core::objects::{AssetEntry, DexAssetPairing};
use abstract_core::PROXY;
use autocompounder::kujira_tx::TOKEN_FACTORY_CREATION_FEE;
use cw_orch::daemon::networks::osmosis::OSMO_NETWORK;
use cw_orch::daemon::queriers::Bank;
use cw_orch::daemon::{ChainInfo, ChainKind, DaemonBuilder};
use cw_orch::prelude::*;
use std::env;

use abstract_core::objects::price_source::UncheckedPriceSource;
use abstract_core::proxy;
use abstract_core::proxy::{AssetsConfigResponse, QueryMsgFns};
use abstract_interface::AbstractAccount;
use cw_utils::Duration;
use std::sync::Arc;

use clap::Parser;
use cosmwasm_std::{coin, Decimal};
use cw_orch::daemon::networks::parse_network;

use autocompounder::interface::{AutocompounderApp, Vault};
use autocompounder::msg::{AutocompounderInstantiateMsg, AutocompounderQueryMsgFns, BondingData};
use log::info;

// To deploy the app we need to get the memory and then register it
// We can then deploy a test Account that uses that new app

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

const FORTY_TWO_ADMIN_NAMESPACE: &str = "4t2-testing";

const _MODULE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn description(asset_string: String) -> String {
    format!(
        "Within the vault, users {} LP tokens are strategically placed into an Astroport farm, generating the platform governance token as rewards. These earned tokens are intelligently exchanged to acquire additional underlying assets, further boosting the volume of the same liquidity tokens. The newly acquired axlUSDC/ASTRO LP tokens are promptly integrated back into the farm, primed for upcoming earning events. The transaction costs associated with these processes are distributed among the users of the vault, creating a collective and efficient approach.",
        asset_string
    )
}

fn init_vault(args: Arguments) -> anyhow::Result<()> {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());

    let (dex, base_pair_asset, cw20_code_id, token_creation_fee) = match args.network_id.as_str() {
        "uni-6" => ("wyndex", "juno>junox", Some(4012), None),
        "juno-1" => ("wyndex", "juno>juno", Some(1), None),
        "pion-1" => ("astroport", "neutron>astro", Some(188), None),
        "neutron-1" => ("astroport", "neutron>astro", Some(180), None),
        "pisco-1" => ("astroport", "terra2>luna", Some(83), None),
        "phoenix-1" => ("astroport", "terra2>luna", Some(69), None),
        "osmo-test-5" => ("osmosis", "osmosis>osmo", None, None),
        "osmosis-1" => ("osmosis", "osmosis>osmo", None, None),
        "harpoon-4" => (
            "kujira",
            "kujira>kuji",
            None,
            Some(TOKEN_FACTORY_CREATION_FEE),
        ),
        _ => panic!("Unknown network id: {}", args.network_id),
    };

    info!("Using dex: {} and base: {}", dex, base_pair_asset);

    let network: ChainInfo = if &args.network_id == "osmosis-1" {
        // Override osmosis 1 config temporarily
        OSMOSIS_1
    } else {
        parse_network(&args.network_id).unwrap()
    };

    let chain = DaemonBuilder::default()
        .handle(rt.handle())
        .chain(network)
        .build()?;

    let sender = chain.sender();

    let abstr_client = AbstractClient::new(chain.clone())?;

    let parent_account = get_parent_account(&abstr_client)?;

    let mut pair_assets = vec![args.paired_asset.clone(), args.other_asset.clone()];
    pair_assets.sort();

    let human_readable_assets = pair_assets.join("|").replace('>', ":");
    let vault_name = format!("4t2 Vault ({})", human_readable_assets);

    if !args.force_new {
        check_for_existing_vaults(parent_account.clone(), vault_name.clone())?;
    }

    // Funds for creating the token denomination
    let instantiation_funds: Vec<Coin> = if let Some(creation_fee) = token_creation_fee {
        let bank: Bank = chain.querier();
        let balance: u128 = bank.balance(&sender, Some("ukuji".to_string())).unwrap()[0]
            .amount
            .u128();
        if balance < creation_fee {
            panic!("Not enough ukuji to pay for token factory creation fee");
        }
        vec![coin(creation_fee, "ukuji")]
    } else {
        vec![]
    };

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

    let autocompounder_instantiate_msg = AutocompounderInstantiateMsg {
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
        pool_assets: pair_assets.clone().into_iter().map(Into::into).collect(),
        bonding_data,
        max_swap_spread: Some(Decimal::percent(10)),
    };
    let new_vault_account = abstr_client
        .account_builder()
        .sub_account(&parent_account)
        .name(format!(
            "TESTING 4t2 Vault ({})",
            pair_assets.join("|").replace('>', ":")
        ))
        .description(description(pair_assets.join("|").replace('>', ":")))
        .build()?;

    info!("Instantiated AC addr: {}", new_vault_account.manager()?);

    let new_vault_account_id = new_vault_account.id()?;

    println!("New vault account id: {}", new_vault_account_id);

    new_vault_account.install_app_with_dependencies::<AutocompounderApp<_>>(
        &autocompounder_instantiate_msg,
        Empty {},
        &instantiation_funds,
    )?;

    // Osmosis does not support value calculation via pools
    if dex != "osmosis" {
        let base_asset: AssetEntry = base_pair_asset.into();
        let paired_asset: AssetEntry = if args.paired_asset == *base_pair_asset.to_string() {
            args.other_asset.into()
        } else {
            args.paired_asset.into()
        };

        register_assets(
            new_vault_account.as_ref(),
            base_asset,
            paired_asset,
            dex,
            pair_assets,
        )?;
    }

    let new_vault = Vault::new(new_vault_account.as_ref())?;
    let installed_modules = new_vault_account.module_infos()?;
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

fn check_for_existing_vaults<Env: CwEnv>(
    parent_account: Account<Env>,
    vault_name: String,
) -> Result<(), anyhow::Error> {
    // check all sub-accounts for the same vault
    let sub_accounts = parent_account.sub_accounts()?;
    for sub_account in sub_accounts {
        let sub_account_name = sub_account.info()?.name;
        if sub_account_name == vault_name {
            panic!(
                "Found existing vault: {} with same assets as vault id: {}",
                vault_name,
                sub_account.id()?
            );
        }
    }
    Ok(())
}

/// Register the assets on the account for value calculation
fn register_assets<Env: CwEnv>(
    vault_account: &AbstractAccount<Env>,
    base_pair_asset: AssetEntry,
    paired_asset: AssetEntry,
    dex: &str,
    vault_pair_assets: Vec<String>,
) -> Result<(), anyhow::Error> {
    let AssetsConfigResponse {
        assets: actual_registered_assets,
    } = vault_account.proxy.assets_config(None, None)?;

    let actual_registered_entries = actual_registered_assets
        .into_iter()
        .map(|(asset, _)| asset.clone())
        .collect::<Vec<_>>();

    // Register the assets on the vault
    let expected_registered_assets = vec![
        (base_pair_asset, UncheckedPriceSource::None),
        (
            paired_asset,
            UncheckedPriceSource::Pair(DexAssetPairing::new(
                vault_pair_assets[0].clone().into(),
                vault_pair_assets[1].clone().into(),
                dex,
            )),
        ),
    ];

    let assets_to_register = expected_registered_assets
        .into_iter()
        .filter(|(asset, _)| !actual_registered_entries.contains(asset))
        .map(|(asset, price_source)| (asset.clone(), price_source.clone()))
        .collect::<Vec<_>>();

    println!("Registering assets: {:?}", assets_to_register);

    vault_account.manager.execute_on_module(
        PROXY,
        proxy::ExecuteMsg::UpdateAssets {
            to_add: assets_to_register,
            to_remove: vec![],
        },
    )?;
    Ok(())
}

/// Retrieve the account that will have all the 4t2 vaults as sub-accounts
fn get_parent_account<Chain: CwEnv>(
    client: &AbstractClient<Chain>,
) -> Result<Account<Chain>, anyhow::Error> {
    let account = client
        .account_builder()
        .name("fortytwo manager")
        .namespace(Namespace::unchecked(FORTY_TWO_ADMIN_NAMESPACE))
        .description("manager of 4t2 smartcontracts")
        .build()?;

    Ok(account)
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
