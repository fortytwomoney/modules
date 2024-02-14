use std::str::FromStr;

use abstract_client::{AbstractClient, Namespace};
use abstract_client::{Account, Application, Publisher};
use abstract_cw_staking::interface::CwStakingAdapter;
use abstract_dex_adapter::interface::DexAdapter;
use abstract_dex_adapter::msg::DexInstantiateMsg;
use autocompounder::interface::AutocompounderApp;
use cosmwasm_std::Decimal;
use cw_orch::contract::interface_traits::ContractInstance;
use cw_orch::contract::interface_traits::CwOrchExecute;
use cw_orch::environment::CwEnv;

use super::TEST_NAMESPACE;

/// Sets up the autocompounder account, including adapters and adapter registration
#[allow(dead_code)]
pub fn setup_autocompounder_account<Chain: CwEnv>(
    abstract_client: &AbstractClient<Chain>,
    autocompounder_instantiate_msg: &autocompounder::msg::AutocompounderInstantiateMsg,
) -> anyhow::Result<(
    DexAdapter<Chain>,
    CwStakingAdapter<Chain>,
    Publisher<Chain>,
    Account<Chain>,
    Application<Chain, AutocompounderApp<Chain>>,
)> {
    let abstract_publisher = abstract_client
        .publisher_builder(Namespace::new("abstract")?)
        .build()?;

    let dex_adapter: DexAdapter<_> = abstract_publisher.publish_adapter(DexInstantiateMsg {
        swap_fee: Decimal::from_str("0.003")?,
        recipient_account: 0,
    })?;
    let staking_adapter: CwStakingAdapter<_> =
        abstract_publisher.publish_adapter(cosmwasm_std::Empty {})?;

    let fortytwo_publisher = abstract_client
        .publisher_builder(Namespace::new(TEST_NAMESPACE)?)
        .build()?;

    fortytwo_publisher.publish_app::<AutocompounderApp<_>>()?;

    let account = abstract_client
        .account_builder()
        .install_on_sub_account(true)
        .build()?;

    let autocompounder_app = account.install_app_with_dependencies::<AutocompounderApp<_>>(
        autocompounder_instantiate_msg,
        cosmwasm_std::Empty {},
        &[],
    )?;

    dex_adapter.execute(
        &abstract_dex_adapter::msg::ExecuteMsg::Base(abstract_core::adapter::BaseExecuteMsg {
            proxy_address: Some(autocompounder_app.account().proxy()?.to_string()),
            msg: abstract_core::adapter::AdapterBaseMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_app.addr_str()?],
                to_remove: vec![],
            },
        }),
        None,
    )?;
    staking_adapter.execute(
        &abstract_cw_staking::msg::ExecuteMsg::Base(abstract_core::adapter::BaseExecuteMsg {
            proxy_address: Some(autocompounder_app.account().proxy()?.to_string()),
            msg: abstract_core::adapter::AdapterBaseMsg::UpdateAuthorizedAddresses {
                to_add: vec![autocompounder_app.addr_str()?],
                to_remove: vec![],
            },
        }),
        None,
    )?;

    Ok((
        dex_adapter,
        staking_adapter,
        fortytwo_publisher,
        account,
        autocompounder_app,
    ))
}
