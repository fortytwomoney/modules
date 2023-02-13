use cosmwasm_std::{CosmosMsg, Decimal, Deps, DepsMut, Env, Reply, Response, StdResult, SubMsg, Uint128};
use abstract_sdk::{
    os::{
        dex::OfferAsset,
        objects::AnsAsset
    },
    apis::dex::{Dex, DexInterface},
    apis::respond::AbstractResponse,
    os::objects::{AssetEntry, LpToken, PoolMetadata},
    ModuleInterface,
    Resolve,
    base::features::{AbstractNameService, Identification}
};
use cw_asset::{Asset, AssetInfo};
use forty_two::{
    autocompounder::Config,
    cw_staking::{CW_STAKING, CwStakingQueryMsg, RewardTokensResponse}
};
use crate::{
    contract::{AutocompounderApp, AutocompounderResult, CP_PROVISION_REPLY_ID, FEE_SWAPPED_REPLY, SWAPPED_REPLY_ID},
    error::AutocompounderError,
    state::{CONFIG, FEE_CONFIG}
};

pub fn lp_compound_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let dex = app.dex(deps.as_ref(), config.pool_data.dex.clone());

    let fee_config = FEE_CONFIG.load(deps.storage)?;

    // 1) claim rewards (this happened in the execution before this reply)

    // 2.1) query the rewards
    let mut rewards = get_staking_rewards(deps.as_ref(), &app, &config)?;

    // 2) deduct fee from rewards
    let fees = rewards
        .iter_mut()
        .map(|reward| -> StdResult<AnsAsset> {
            let fee = reward.amount * fee_config.performance;

            reward.amount -= fee;

            Ok(AnsAsset::new(reward.name.clone(), fee))
        })
        .collect::<StdResult<Vec<AnsAsset>>>()?;

    // 3) (swap and) Send fees to treasury
    let (fee_swap_msgs, fee_swap_submsg) =
        swap_rewards_with_reply(fees, vec![fee_config.fee_asset], &dex, FEE_SWAPPED_REPLY)?;

    // 3) Swap rewards to token in pool
    // 3.1) check if asset is not in pool assets
    let pool_assets = config.pool_data.assets;
    if rewards.iter().all(|f| pool_assets.contains(&f.name)) {
        // 3.1.1) if all assets are in the pool, we can just provide liquidity
        //  TODO: but we might need to check the length of the rewards.


        // The liquditiy assets are all the pool assets with the amount of the rewards
        let liquidity_assets = pool_assets
            .iter()
            .map(|pool_asset| -> AnsAsset {
                // Get the amount of the reward or return 0
                let amount = rewards
                    .iter()
                    .find(|reward| reward.name == *pool_asset)
                    .map(|reward| reward.amount)
                    .unwrap_or(Uint128::zero());
                OfferAsset::new(pool_asset.clone(), amount)
            })
            .collect::<Vec<OfferAsset>>();

        // 3.1.2) provide liquidity
        let lp_msg: CosmosMsg = dex.provide_liquidity(liquidity_assets, Some(Decimal::percent(50)))?;

        let submsg = SubMsg::reply_on_success(lp_msg, CP_PROVISION_REPLY_ID);

        let response = Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_submessage(submsg);
        Ok(app.tag_response(response, "provide_liquidity"))
    } else {
        let (swap_msgs, submsg) =
            swap_rewards_with_reply(rewards, pool_assets, &dex, SWAPPED_REPLY_ID)?;

        // adds all swap messages to the response and the submsg -> the submsg will be executed after the last swap message
        // and will trigger the reply SWAPPED_REPLY_ID
        let response = Response::new()
            .add_messages(fee_swap_msgs)
            .add_submessage(fee_swap_submsg)
            .add_messages(swap_msgs)
            .add_submessage(submsg);
        Ok(app.tag_response(response, "swap_rewards"))
    }
    // TODO: stake lp tokens
}

pub fn query_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    pool_data: PoolMetadata,
) -> StdResult<Vec<AssetInfo>> {
    // query staking module for which rewards are available
    let modules = app.modules(deps);
    let query = CwStakingQueryMsg::RewardTokens {
        provider: pool_data.dex.clone(),
        staking_token: LpToken::from(pool_data).into(),
    };
    let RewardTokensResponse { tokens } = modules.query_api(CW_STAKING, query)?;

    Ok(tokens)
}

/// swaps all rewards that are not in the target assets and add a reply id to the latest swapmsg
fn swap_rewards_with_reply(
    rewards: Vec<AnsAsset>,
    target_assets: Vec<AssetEntry>,
    dex: &Dex<AutocompounderApp>,
    reply_id: u64,
) -> Result<(Vec<CosmosMsg>, SubMsg), AutocompounderError> {
    let mut swap_msgs: Vec<CosmosMsg> = vec![];
    rewards
        .iter()
        .try_for_each(|reward: &AnsAsset| -> StdResult<_> {
            if !target_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = dex.swap(
                    reward.clone(),
                    target_assets.get(0).unwrap().clone(),
                    Some(Decimal::percent(50)),
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

/// queries available staking rewards assets and the corresponding balances
fn get_staking_rewards(
    deps: Deps,
    app: &AutocompounderApp,
    config: &Config,
) -> StdResult<Vec<AnsAsset>> {
    let ans_host = app.ans_host(deps)?;
    let rewards = query_rewards(deps, app, config.pool_data.clone())?;
    // query balance of rewards
    let rewards = rewards
        .into_iter()
        .map(|tkn| -> StdResult<Asset> {
            // 2) get the number of LP tokens minted in this transaction
            let balance = tkn.query_balance(&deps.querier, app.proxy_address(deps)?)?;
            Ok(Asset::new(tkn, balance))
        })
        .collect::<StdResult<Vec<Asset>>>()?;
    // resolve rewards to AnsAssets for dynamic processing (swaps)
    let rewards = rewards
        .into_iter()
        .filter(|reward| reward.amount != Uint128::zero())
        .map(|asset| asset.resolve(&deps.querier, &ans_host))
        .collect::<Result<Vec<AnsAsset>, _>>()?;
    Ok(rewards)
}
