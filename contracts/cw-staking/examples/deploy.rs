use abstract_boot::{AnsHost, Deployment, DexApi, ModuleDeployer, VersionControl};
use boot_core::networks::UNI_5;
use boot_core::prelude::instantiate_daemon_env;
use boot_core::prelude::*;
use boot_core::DaemonOptionsBuilder;
use cosmwasm_std::{Addr, Empty};
use semver::Version;
use std::sync::Arc;
use tokio::runtime::Runtime;
use forty_two::cw_staking::CW_STAKING;
use forty_two_boot::cw_staking::CwStakingApi;

const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn deploy_cw_staking() -> anyhow::Result<()> {
    let version: Version = CONTRACT_VERSION.parse().unwrap();
    let network = UNI_5;

    let rt = Arc::new(Runtime::new()?);
    let options = DaemonOptionsBuilder::default().network(network).build();
    let (_sender, chain) = instantiate_daemon_env(&rt, options?)?;

    let abstract_version: Version = "0.1.0-rc.3".parse().unwrap();
    let mut deployer = ModuleDeployer::load_from_version_control(
        &chain,
        &abstract_version,
        &Addr::unchecked("juno102k70cekzkwgex55en0zst5gy9x5h3gf8cegvn76w2uevqj4wdgs0q67mq"),
    )?;

    let mut cw_staking = CwStakingApi::new(CW_STAKING, &chain);

    deployer.deploy_api(cw_staking.as_instance_mut(), version, Empty {})?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    dotenv().ok();
    env_logger::init();

    use dotenv::dotenv;

    deploy_cw_staking()
}
