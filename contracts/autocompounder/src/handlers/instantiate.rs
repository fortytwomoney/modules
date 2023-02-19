use crate::contract::{AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID};
use crate::error::AutocompounderError;
use crate::handlers::helpers::check_fee;
use crate::state::{Config, CONFIG, FEE_CONFIG};
use abstract_sdk::ApiInterface;
use abstract_sdk::{
    features::AbstractNameService,
    os::api,
    os::objects::{AssetEntry, DexAssetPairing, LpToken, PoolReference},
    Resolve,
};
use cosmwasm_std::{
    to_binary, Addr, Deps, DepsMut, Env, MessageInfo, ReplyOn, Response, StdError, StdResult,
    SubMsg, WasmMsg,
};
use cw20::MinterResponse;
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
use cw_staking::{
    msg::{CwStakingQueryMsg, StakingInfoResponse},
    CW_STAKING,
};
use cw_utils::Duration;
use forty_two::autocompounder::{
    AutocompounderInstantiateMsg, BondingPeriodSelector, FeeConfig, AUTOCOMPOUNDER,
};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderInstantiateMsg,
) -> AutocompounderResult {
    let ans = app.name_service(deps.as_ref());

    let ans_host = app.ans_host(deps.as_ref())?;

    let AutocompounderInstantiateMsg {
        performance_fees,
        deposit_fees,
        withdrawal_fees,
        fee_asset,
        commission_addr,
        code_id,
        dex,
        pool_assets,
        preferred_bonding_period,
    } = msg;

    check_fee(performance_fees)?;
    check_fee(deposit_fees)?;
    check_fee(withdrawal_fees)?;

    if pool_assets.len() > 2 {
        return Err(AutocompounderError::PoolWithMoreThanTwoAssets {});
    }

    // verify that pool assets are valid
    pool_assets.resolve(&deps.querier, &ans_host)?;

    let lp_token = LpToken {
        dex: dex.clone(),
        assets: pool_assets.clone(),
    };
    let lp_token_info = ans.query(&lp_token)?;

    // match on the info and get cw20
    let lp_token_addr: Addr = match lp_token_info {
        cw_asset::AssetInfoBase::Cw20(addr) => Ok(addr),
        _ => Err(AutocompounderError::Std(StdError::generic_err(
            "LP token is not a cw20",
        ))),
    }?;

    let pool_assets_slice = &mut [&pool_assets[0].clone(), &pool_assets[1].clone()];

    // sort pool_assets then join
    // staking/astroport/crab,juno
    // let staking_contract_name = ["staking", &lp_token.to_string()].join("/");
    // let staking_contract_entry = UncheckedContractEntry::new(&dex, staking_contract_name).check();
    // let staking_contract_addr = ans.query(&staking_contract_entry)?;

    // get staking info
    let staking_info = query_staking_info(deps.as_ref(), &app, lp_token.into(), dex.clone())?;
    let (unbonding_period, min_unbonding_cooldown) =
        if let (max_claims, Some(mut unbonding_periods)) =
            (staking_info.max_claims, staking_info.unbonding_periods)
        {
            unbonding_periods.sort_by(|a, b| {
                if let (Duration::Height(a), Duration::Height(b)) = (a, b) {
                    a.cmp(b)
                } else if let (Duration::Time(a), Duration::Time(b)) = (a, b) {
                    a.cmp(b)
                } else {
                    panic!("Unbonding periods are not all heights or all times")
                }
            });
            let unbonding_duration = match preferred_bonding_period {
                BondingPeriodSelector::Shortest => *unbonding_periods.first().unwrap(),
                BondingPeriodSelector::Longest => *unbonding_periods.last().unwrap(),
                BondingPeriodSelector::Custom(duration) => {
                    // check if the duration is in the unbonding periods
                    if unbonding_periods.contains(&duration) {
                        duration
                    } else {
                        return Err(AutocompounderError::Std(StdError::generic_err(
                            "Custom bonding period is not in the dex's unbonding periods",
                        )));
                    }
                }
            };
            let min_unbonding_cooldown = max_claims.map(|max| match &unbonding_duration {
                Duration::Height(block) => Duration::Height(block.saturating_div(max.into())),
                Duration::Time(secs) => Duration::Time(secs.saturating_div(max.into())),
            });
            (Some(unbonding_duration), min_unbonding_cooldown)
        } else {
            (None, None)
        };

    // TODO: Store this in the config
    let pairing = DexAssetPairing::new(
        pool_assets_slice[0].clone(),
        pool_assets_slice[1].clone(),
        &dex,
    );
    let mut pool_references = pairing.resolve(&deps.querier, &ans_host)?;

    assert_eq!(pool_references.len(), 1);
    // Takes the value from the vector
    let pool_reference: PoolReference = pool_references.swap_remove(0);
    // get the pool data
    let pool_data = pool_reference.unique_id.resolve(&deps.querier, &ans_host)?;

    // TODO: use ResolvedPoolMetadata
    let resolved_pool_assets = pool_data.assets.resolve(&deps.querier, &ans_host)?;

    let config: Config = Config {
        vault_token: Addr::unchecked(""),
        staking_contract: staking_info.staking_contract_address,
        liquidity_token: lp_token_addr,
        pool_data,
        pool_assets: resolved_pool_assets,
        pool_address: pool_reference.pool_address,
        unbonding_period,
        min_unbonding_cooldown,
    };

    CONFIG.save(deps.storage, &config)?;

    let fee_config = FeeConfig {
        performance: performance_fees,
        deposit: deposit_fees,
        withdrawal: withdrawal_fees,
        fee_asset: AssetEntry::from(fee_asset),
        commission_addr: deps.api.addr_validate(&commission_addr)?,
    };

    FEE_CONFIG.save(deps.storage, &fee_config)?;

    // create LP token SubMsg
    let sub_msg = create_lp_token_submsg(
        env.contract.address.to_string(),
        format!("4T2{pairing}"),
        // pool data is too long
        // format!("4T2 Vault Token for {pool_data}"),
        "FTTV".to_string(), // TODO: find a better way to define name and symbol
        code_id,
    )?;

    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}

/// create a SubMsg to instantiate the Vault token.
fn create_lp_token_submsg(
    minter: String,
    name: String,
    symbol: String,
    code_id: u64,
) -> Result<SubMsg, StdError> {
    let msg = TokenInstantiateMsg {
        name,
        symbol,
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse { minter, cap: None }),
        marketing: None,
    };
    Ok(SubMsg {
        msg: WasmMsg::Instantiate {
            admin: None,
            code_id,
            msg: to_binary(&msg)?,
            funds: vec![],
            label: "4T2 Vault Token".to_string(),
        }
        .into(),
        gas_limit: None,
        id: INSTANTIATE_REPLY_ID,
        reply_on: ReplyOn::Success,
    })
}

pub fn query_staking_info(
    deps: Deps,
    app: &AutocompounderApp,
    lp_token_name: AssetEntry,
    dex: String,
) -> StdResult<StakingInfoResponse> {
    let apis = app.apis(deps);

    let query = CwStakingQueryMsg::Info {
        provider: dex.clone(),
        staking_token: lp_token_name.clone(),
    };

    let api_msg: api::QueryMsg<_> = query.clone().into();

    let res: StakingInfoResponse = apis.query(CW_STAKING, query).map_err(|e| {
        StdError::generic_err(format!(
            "Error querying staking info for {lp_token_name} on {dex}: {e}...{api_msg:?}"
        ))
    })?;
    Ok(res)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_common::app_init;
    use cosmwasm_std::{Addr, Decimal};
    use cw_asset::AssetInfo;
    use speculoos::assert_that;

    #[test]
    fn test_app_instantiation() -> anyhow::Result<()> {
        let deps = app_init();
        let config = CONFIG.load(deps.as_ref().storage).unwrap();
        let fee_config = FEE_CONFIG.load(deps.as_ref().storage).unwrap();
        assert_that!(config.pool_assets).is_equal_to(vec![
            AssetInfo::Native("usd".into()),
            AssetInfo::Native("eur".into()),
        ]);
        assert_that!(fee_config).is_equal_to(FeeConfig {
            performance: Decimal::percent(3),
            deposit: Decimal::percent(3),
            withdrawal: Decimal::percent(3),
            fee_asset: "eur".to_string().into(),
            commission_addr: Addr::unchecked("commission_receiver".to_string()),
        });
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
