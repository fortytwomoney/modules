use abstract_client::{AbstractClient, Namespace};
use abstract_core::manager::ModuleInstallConfig;
use abstract_core::objects::module::ModuleVersion;
use abstract_core::objects::{AccountId, AssetEntry, DexAssetPairing, PoolMetadata};
use autocompounder::kujira_tx::TOKEN_FACTORY_CREATION_FEE;
use cw_orch::daemon::networks::osmosis::OSMO_NETWORK;
use cw_orch::daemon::{ChainInfo, ChainKind, DaemonBuilder};
use cw_orch::deploy::Deploy;
use cw_orch::prelude::queriers::{Bank, DaemonQuerier};
use cw_orch::prelude::*;
use std::env;

use abstract_core::ans_host::ExecuteMsgFns;
use abstract_core::objects::pool_id::PoolAddressBase;
use abstract_core::objects::price_source::UncheckedPriceSource;
use abstract_core::proxy::{AssetsConfigResponse, QueryMsgFns};
use abstract_core::{
    app, manager, objects::gov_type::GovernanceDetails, objects::module::ModuleInfo, proxy,
    registry::ANS_HOST, OSMOSIS, PROXY,
};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_cw_staking::CW_STAKING_ADAPTER_ID;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_dex_adapter::msg::DexInstantiateMsg;
use abstract_dex_adapter::DEX_ADAPTER_ID;
use abstract_interface::{
    Abstract, AbstractAccount, AccountDetails, AdapterDeployer, AppDeployer, DeployStrategy,
    ManagerQueryFns,
};
use cw_utils::Duration;
use std::sync::Arc;

use clap::Parser;
use cosmwasm_std::{coin, coins, Addr, Decimal};
use cw_orch::daemon::networks::parse_network;

use autocompounder::interface::{AutocompounderApp, Vault};
use autocompounder::msg::{
    AutocompounderInstantiateMsg, AutocompounderQueryMsgFns, BondingData, AUTOCOMPOUNDER_ID,
};
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

const FORTY_TWO_ADMIN_NAMESPACE: &'static str = "4t2-testing";

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

    // let abstr = Abstract::deploy_on(chain.clone(), "".into())?;

    // let (chain, _, _, test_parent) = setup_test_tube()?;
    //
    // let parent_account_id = Some(test_parent.id()?.seq());
    //

    let sender = chain.sender();

    let abstr = Abstract::load_from(chain.clone())?;
    let abstr_client = AbstractClient::new(chain.clone())?;

    let parent_account = get_parent_account(&abstr, &abstr_client)?;

    // panic!("here");

    let mut pair_assets = vec![args.paired_asset.clone(), args.other_asset.clone()];
    pair_assets.sort();

    let human_readable_assets = pair_assets.join("|").replace('>', ":");
    let vault_name = format!("4t2 Vault ({})", human_readable_assets);

    if !args.force_new {
        check_for_existing_vaults(&abstr, parent_account.clone(), vault_name.clone())?;
    }

    // Funds for creating the token denomination
    let instantiation_funds: Option<Vec<Coin>> = if let Some(creation_fee) = token_creation_fee {
        // let bank = Bank::new(chain.channel());
        // let balance: u128 = rt
        //     .block_on(bank.balance(&sender, Some("ukuji".to_string())))
        //     .unwrap()[0]
        //     .amount
        //     .parse()?;
        // if balance < creation_fee {
        //     panic!("Not enough ukuji to pay for token factory creation fee");
        // }
        // Some(vec![coin(creation_fee, "ukuji")])
        // Some(vec![coin(creation_fee, "ukuji")])
        None
    } else {
        None
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
        pool_assets: pair_assets.clone().into_iter().map(Into::into).collect(),
        bonding_data,
        max_swap_spread: Some(Decimal::percent(10)),
    };

    let autocompounder_instantiate_msg = &autocompounder_mod_init_msg;

    let manager_create_sub_account_msg = manager::ExecuteMsg::CreateSubAccount {
        base_asset: None,
        namespace: None,
        description: Some(description(pair_assets.join("|").replace('>', ":"))),
        link: None,
        name: format!(
            "TESTING 4t2 Vault ({})",
            pair_assets.join("|").replace('>', ":")
        ),
        install_modules: vec![
            // installs both abstract dex and staking in the instantiation of the account
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(DEX_ADAPTER_ID)?, None),
            ModuleInstallConfig::new(ModuleInfo::from_id_latest(CW_STAKING_ADAPTER_ID)?, None),
        ],
    };

    let result = parent_account.manager.execute(
        &manager_create_sub_account_msg,
        instantiation_funds.as_deref(),
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

    let new_vault_account = AbstractAccount::new(&abstr, AccountId::local(new_vault_account_id));
    println!("New vault account id: {:?}", new_vault_account.id()?);

    new_vault_account.manager.install_module_version(
        AUTOCOMPOUNDER_ID,
        ModuleVersion::Version("0.9.7-test".into()),
        Some(&autocompounder_instantiate_msg),
        None,
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
            &new_vault_account,
            base_asset,
            paired_asset,
            dex,
            pair_assets,
        )?;
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

const ASSET_1: &str = "uosmo";
const ASSET_2: &str = "uatom";
pub const LP: &str = "osmosis/atom,osmo";

fn get_pool_token(id: u64) -> String {
    format!("gamm/pool/{}", id)
}

fn setup_test_tube() -> anyhow::Result<(
    OsmosisTestTube,
    u64,
    CwStakingAdapter<OsmosisTestTube>,
    AbstractAccount<OsmosisTestTube>,
)> {
    let tube = OsmosisTestTube::new(vec![
        coin(1_000_000_000_000, ASSET_1),
        coin(1_000_000_000_000, ASSET_2),
    ]);

    let deployment = Abstract::deploy_on(tube.clone(), tube.sender().to_string())?;

    let _root_os =
        deployment
            .account_factory
            .create_default_account(GovernanceDetails::Monarchy {
                monarch: Addr::unchecked(deployment.account_factory.get_chain().sender())
                    .to_string(),
            })?;

    // Deploy staking adatper
    let staking: CwStakingAdapter<OsmosisTestTube> =
        CwStakingAdapter::new("abstract:cw-staking", tube.clone());

    staking.deploy("0.19.2".parse()?, Empty {}, DeployStrategy::Try)?;

    // deploy dex adapter
    let dex: DexAdapter<OsmosisTestTube> = DexAdapter::new("abstract:dex", tube.clone());
    dex.deploy(
        "0.19.2".parse()?,
        DexInstantiateMsg {
            swap_fee: Default::default(),
            recipient_account: 0,
        },
        DeployStrategy::Try,
    )?;

    let autocompounder: AutocompounderApp<OsmosisTestTube> =
        AutocompounderApp::new("4t2:autocompounder", tube.clone());
    autocompounder.deploy("1.2.3".parse()?, DeployStrategy::Try)?;

    let os = deployment
        .account_factory
        .create_default_account(GovernanceDetails::Monarchy {
            monarch: Addr::unchecked(deployment.account_factory.get_chain().sender()).to_string(),
        })?;
    let _manager_addr = os.manager.address()?;

    // transfer some LP tokens to the AbstractAccount, as if it provided liquidity
    let pool_id = tube.create_pool(vec![coin(1_000, ASSET_1), coin(1_000, ASSET_2)])?;

    deployment
        .ans_host
        .update_asset_addresses(
            vec![
                ("osmo".to_string(), cw_asset::AssetInfoBase::native(ASSET_1)),
                ("atom".to_string(), cw_asset::AssetInfoBase::native(ASSET_2)),
                (
                    LP.to_string(),
                    cw_asset::AssetInfoBase::native(get_pool_token(pool_id)),
                ),
            ],
            vec![],
        )
        .unwrap();

    deployment
        .ans_host
        .update_dexes(vec!["osmosis".into()], vec![])
        .unwrap();

    deployment
        .ans_host
        .update_pools(
            vec![(
                PoolAddressBase::id(pool_id),
                PoolMetadata::constant_product(
                    "osmosis",
                    vec!["atom".to_string(), "osmo".to_string()],
                ),
            )],
            vec![],
        )
        .unwrap();

    // install exchange on AbstractAccount
    os.install_adapter(&staking, None)?;

    tube.bank_send(
        os.proxy.addr_str()?,
        coins(1_000u128, get_pool_token(pool_id)),
    )?;

    Ok((tube, pool_id, staking, os))
}

fn check_for_existing_vaults<Env: CwEnv>(
    abstr: &Abstract<Env>,
    parent_account: AbstractAccount<Env>,
    vault_name: String,
) -> Result<(), anyhow::Error> {
    // check all sub-accounts for the same vault
    let mut start_after: Option<u32> = None;

    loop {
        let sub_account_batch = parent_account
            .manager
            .sub_account_ids(None, start_after)?
            .sub_accounts;

        if sub_account_batch.is_empty() {
            break;
        }

        for sub_account_id in sub_account_batch.clone() {
            let sub_account = AbstractAccount::new(abstr, AccountId::local(sub_account_id));
            let sub_account_name = sub_account.manager.info()?.info.name;

            if sub_account_name == vault_name {
                panic!(
                    "Found existing vault: {} with same assets as vault id: {}",
                    vault_name, sub_account_id
                );
            }
        }

        start_after = sub_account_batch.last().map(|i| i.to_owned());
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
    abstr: &Abstract<Chain>,
    client: &AbstractClient<Chain>,
) -> Result<AbstractAccount<Chain>, anyhow::Error> {
    let account = client
        .account_builder()
        .name("fortytwo manager")
        .namespace(Namespace::unchecked(FORTY_TWO_ADMIN_NAMESPACE))
        .description("manager of 4t2 smartcontracts")
        .ownership(GovernanceDetails::Monarchy {
            monarch: client.sender().to_string(),
        })
        .install_on_sub_account(true)
        .build()?;

    Ok(AbstractAccount::new(&abstr, account.id()?))
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
