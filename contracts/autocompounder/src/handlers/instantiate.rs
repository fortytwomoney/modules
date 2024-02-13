use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::handlers::helpers::check_fee;
use crate::kujira_tx::format_tokenfactory_denom;
use crate::msg::{AutocompounderInstantiateMsg, FeeConfig, AUTOCOMPOUNDER};
use crate::state::{Config, CONFIG, DEFAULT_MAX_SPREAD, FEE_CONFIG};
use abstract_core::objects::{AnsEntryConvertor, AssetEntry};
use abstract_cw_staking::msg::{StakingInfoResponse, StakingQueryMsg};
use abstract_cw_staking::CW_STAKING_ADAPTER_ID;
use abstract_sdk::{
    core::objects::{LpToken, PoolReference},
    features::AbstractNameService,
};
use abstract_sdk::{AbstractResponse, AdapterInterface};
use cosmwasm_std::{Addr, Decimal, DepsMut, Env, MessageInfo};
use cw_asset::AssetInfo;

use super::helpers::{
    create_subdenom_from_pool_assets, create_vault_token_submsg, get_unbonding_period_and_cooldown,
};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderInstantiateMsg,
) -> AutocompounderResult {
    // load abstract name service
    let ans = app.name_service(deps.as_ref());

    let AutocompounderInstantiateMsg {
        performance_fees,
        deposit_fees,
        withdrawal_fees,
        commission_addr,
        code_id,
        dex,
        pool_assets,
        bonding_data: manual_bonding_data,
        max_swap_spread,
    } = msg;

    check_fee(performance_fees)?;
    check_fee(deposit_fees)?;
    check_fee(withdrawal_fees)?;

    if pool_assets.len() > 2 {
        return Err(AutocompounderError::PoolWithMoreThanTwoAssets {});
    }

    let lp_token = LpToken::new(dex.clone(), pool_assets.clone());
    let lp_asset: AssetEntry = AnsEntryConvertor::new(lp_token.clone()).asset_entry();
    let pairing = AnsEntryConvertor::new(lp_token.clone()).dex_asset_pairing()?;

    let staking_info: StakingInfoResponse =
        app.adapters(deps.as_ref()).query::<StakingQueryMsg, _>(
            CW_STAKING_ADAPTER_ID,
            StakingQueryMsg::Info {
                provider: dex.clone(),
                staking_tokens: vec![lp_asset],
            },
        )?;

    // verify that pool assets are valid
    ans.query(&pool_assets)?;

    let (unbonding_period, min_unbonding_cooldown) =
        get_unbonding_period_and_cooldown(manual_bonding_data)?;

    let mut pool_references = ans.query(&pairing)?;
    let pool_reference: PoolReference = pool_references.swap_remove(0);
    // get the pool data
    let pool_data = ans.query(&pool_reference.unique_id)?;

    let resolved_pool_assets = ans.query(&pool_data.assets)?;

    // default max swap spread
    let max_swap_spread =
        max_swap_spread.unwrap_or_else(|| Decimal::percent(DEFAULT_MAX_SPREAD.into()));

    // vault_token will be overwritten in the instantiate reply if we are using a cw20

    let subdenom = create_subdenom_from_pool_assets(&pool_data);
    let vault_token = if code_id.is_some() {
        AssetInfo::cw20(Addr::unchecked(""))
    } else {
        AssetInfo::Native(format_tokenfactory_denom(
            env.contract.address.as_str(),
            &subdenom,
        ))
    };

    let config: Config = Config {
        vault_token,
        liquidity_token: staking_info.infos[0].staking_token.clone(),
        pool_data,
        pool_assets: resolved_pool_assets,
        pool_address: pool_reference.pool_address,
        unbonding_period,
        min_unbonding_cooldown,
        max_swap_spread,
    };

    CONFIG.save(deps.storage, &config)?;

    let fee_config = FeeConfig {
        performance: performance_fees,
        deposit: deposit_fees,
        withdrawal: withdrawal_fees,
        fee_collector_addr: deps.api.addr_validate(&commission_addr)?,
    };

    FEE_CONFIG.save(deps.storage, &fee_config)?;

    // create LP token SubMsg
    let sub_msg = create_vault_token_submsg(
        env.contract.address.to_string(),
        subdenom,
        code_id, // if code_id is none, submsg will be like normal msg: no reply (for now).
        config.pool_data.dex,
    )?;

    Ok(app
        .response("instantiate")
        .add_submessage(sub_msg)
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}

#[cfg(test)]
mod test {
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_base_mock_querier};
    use abstract_core::objects::{AssetEntry, DexAssetPairing};
    use abstract_core::version_control::AccountBase;
    use abstract_sdk::base::InstantiateEndpoint;
    use abstract_sdk::core as abstract_core;
    use abstract_testing::prelude::{
        TEST_ANS_HOST, TEST_MANAGER, TEST_MODULE_FACTORY, TEST_PROXY, TEST_VERSION_CONTROL,
    };
    const ASTROPORT: &str = "astroport";
    const COMMISSION_RECEIVER: &str = "commission_receiver";
    use crate::test_common::app_init;
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        Addr, Decimal,
    };
    use cw20::MinterResponse;
    use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
    use cw_asset::AssetInfo;
    use speculoos::{assert_that, result::ResultAssertions};

    use super::*;

    #[test]
    fn test_app_instantiation() -> anyhow::Result<()> {
        let deps = app_init(false, true);
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        let fee_config = FEE_CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.pool_assets.len()).is_equal_to(2);
        assert_that!(&config.pool_assets).matches(|x| {
            x.contains(&AssetInfo::Native("usd".into()))
                && x.contains(&AssetInfo::Native("eur".into()))
        });
        assert_that!(fee_config).is_equal_to(FeeConfig {
            performance: Decimal::percent(3),
            deposit: Decimal::percent(3),
            withdrawal: Decimal::percent(3),
            fee_collector_addr: Addr::unchecked("commission_receiver".to_string()),
        });
        assert_that!(&config.vault_token).matches(|v| matches!(v, AssetInfo::Cw20(_)));

        // test native token factory asset
        let deps = app_init(false, false);
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        let expected_subdenom = create_subdenom_from_pool_assets(&config.pool_data);
        assert_that!(config.vault_token).is_equal_to(AssetInfo::Native(format_tokenfactory_denom(
            "cosmos2contract",
            &expected_subdenom,
        )));
        Ok(())
    }

    #[test]
    fn pool_assets_length_cannot_be_greater_than_2() -> anyhow::Result<()> {
        let mut deps = mock_dependencies();
        let info = mock_info(TEST_MODULE_FACTORY, &[]);

        deps.querier = app_base_mock_querier().build();

        let resp = AUTOCOMPOUNDER_APP.instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            abstract_core::app::InstantiateMsg {
                module: crate::msg::AutocompounderInstantiateMsg {
                    code_id: Some(1),
                    commission_addr: COMMISSION_RECEIVER.to_string(),
                    deposit_fees: Decimal::percent(3),
                    dex: ASTROPORT.to_string(),
                    performance_fees: Decimal::percent(3),
                    pool_assets: vec!["eur".into(), "usd".into(), "juno".into()],
                    withdrawal_fees: Decimal::percent(3),
                    bonding_data: None,
                    max_swap_spread: None,
                },
                base: abstract_core::app::BaseInstantiateMsg {
                    version_control_address: TEST_VERSION_CONTROL.to_string(),
                    ans_host_address: TEST_ANS_HOST.to_string(),
                    account_base: AccountBase {
                        manager: Addr::unchecked(TEST_MANAGER),
                        proxy: Addr::unchecked(TEST_PROXY),
                    },
                },
            },
        );

        assert_that!(resp)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::PoolWithMoreThanTwoAssets {}));
        Ok(())
    }

    #[test]
    fn test_cw_20_init() {
        let pairing = DexAssetPairing::new(
            AssetEntry::from("terra2>astro"),
            AssetEntry::from("terra2>luna"),
            "astroport",
        );
        let name = format!("4T2 {pairing}");

        let msg = TokenInstantiateMsg {
            name,
            symbol: "FORTYTWO".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: "".to_string(),
                cap: None,
            }),
            marketing: None,
        };

        msg.validate().unwrap();
    }
}
