use crate::msg::Config;
use crate::state::DECIMAL_OFFSET;
use crate::{
    contract::{AutocompounderApp, AutocompounderResult},
    error::AutocompounderError,
};
use abstract_core::objects::AnsAsset;
use abstract_cw_staking_api::{msg::*, CW_STAKING};
use abstract_dex_api::api::Dex;
use abstract_sdk::feature_objects::AnsHost;
use abstract_sdk::{core::objects::AssetEntry, features::AccountIdentification};
use abstract_sdk::{AbstractSdkResult, ApiInterface};
use cosmwasm_std::{wasm_execute, Addr, CosmosMsg, Decimal, Deps, SubMsg, Uint128, QuerierWrapper};
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;
use cw_utils::Duration;

/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
    unbonding_period: Option<Duration>,
) -> AutocompounderResult<Uint128> {
    let apis = app.apis(deps);

    let query = CwStakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
        unbonding_period,
    };
    let res: StakeResponse = apis.query(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn cw20_total_supply(deps: Deps, config: &Config) -> AutocompounderResult<Uint128> {
    let TokenInfoResponse {
        total_supply: vault_tokens_total_supply,
        ..
    } = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20QueryMsg::TokenInfo {})?;
    Ok(vault_tokens_total_supply)
}

pub fn check_fee(fee: Decimal) -> Result<(), AutocompounderError> {
    if fee > Decimal::percent(99) {
        return Err(AutocompounderError::InvalidFee {});
    }
    Ok(())
}

pub fn check_asset_with_ans(fee_asset: &AssetEntry, ans_host: &AnsHost, querier: &QuerierWrapper) -> Result<(), AutocompounderError> {
    let _ = ans_host.query_asset(querier, fee_asset).map_err(|err| AutocompounderError::SenderIsNotLpToken {  })?;
    Ok(())
}


pub fn mint_vault_tokens(
    config: &Config,
    user_address: Addr,
    mint_amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    let mint_msg = wasm_execute(
        config.vault_token.to_string(),
        &Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        },
        vec![],
    )?
    .into();
    Ok(mint_msg)
}

pub fn stake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    asset: AnsAsset,
    unbonding_period: Option<Duration>,
) -> AbstractSdkResult<CosmosMsg> {
    let apis = app.apis(deps);
    apis.request(
        CW_STAKING,
        CwStakingExecuteMsg {
            provider,
            action: CwStakingAction::Stake {
                staking_token: asset,
                unbonding_period,
            },
        },
    )
}

/// Convert vault tokens to lp assets
pub fn convert_to_assets(shares: Uint128, total_assets: Uint128, total_supply: Uint128) -> Uint128 {
    shares.multiply_ratio(
        total_assets + Uint128::from(1u128),
        total_supply + Uint128::from(10u128).pow(DECIMAL_OFFSET),
    )
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
pub fn convert_to_shares(assets: Uint128, total_assets: Uint128, total_supply: Uint128) -> Uint128 {
    assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(DECIMAL_OFFSET),
        total_assets + Uint128::from(1u128),
    )
}

/// swaps all rewards that are not in the target assets and add a reply id to the latest swapmsg
pub fn swap_rewards_with_reply(
    rewards: Vec<AnsAsset>,
    target_assets: Vec<AssetEntry>,
    dex: &Dex<AutocompounderApp>,
    reply_id: u64,
    max_spread: Decimal,
) -> Result<(Vec<CosmosMsg>, SubMsg), AutocompounderError> {
    let mut swap_msgs: Vec<CosmosMsg> = vec![];
    rewards
        .iter()
        .try_for_each(|reward: &AnsAsset| -> AbstractSdkResult<_> {
            if !target_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = dex.swap(
                    reward.clone(),
                    target_assets.get(0).unwrap().clone(),
                    Some(max_spread),
                    None,
                )?;
                swap_msgs.push(swap_msg);
            }
            Ok(())
        })?;
    let swap_msg = swap_msgs.pop().unwrap();
    let submsg = SubMsg::reply_on_success(swap_msg, reply_id);
    Ok((swap_msgs, submsg))
}
