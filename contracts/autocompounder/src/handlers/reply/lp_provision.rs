use cosmwasm_std::{Addr, CosmosMsg, DepsMut, Env, Reply, Response, Uint128, wasm_execute};
use abstract_sdk::os::objects::{AnsAsset, LpToken};
use abstract_sdk::base::features::{AbstractNameService, Identification};
use abstract_sdk::Resolve;
use abstract_sdk::apis::respond::AbstractResponse;
use cw20::Cw20ExecuteMsg::Mint;
use forty_two::autocompounder::Config;
use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::error::AutocompounderError;
use crate::handlers::helpers::{cw20_total_supply, query_stake};
use crate::handlers::reply;
use crate::state::{CACHED_USER_ADDR, CONFIG};

pub fn lp_provision_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    let proxy_address = app.proxy_address(deps.as_ref())?;
    let ans_host = app.ans_host(deps.as_ref())?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let current_vault_supply = cw20_total_supply(deps.as_ref(), &config)?;

    // 2) Retrieve the number of LP tokens minted/staked.
    let lp_token = LpToken::from(config.pool_data.clone());
    let received_lp = lp_token
        .resolve(&deps.querier, &ans_host)?
        .query_balance(&deps.querier, proxy_address.to_string())?;

    let staked_lp = query_stake(
        deps.as_ref(),
        &app,
        config.pool_data.dex.clone(),
        lp_token.clone().into(),
        config.unbonding_period,
    )?;

    // The increase in LP tokens held by the vault should be reflected by an equal increase (% wise) in vault tokens.
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = compute_mint_amount(staked_lp, current_vault_supply, received_lp);

    // 4) Mint vault tokens to the user
    let mint_msg = mint_vault_tokens(&config, user_address, mint_amount)?;

    // 5) Stake the LP tokens
    let stake_msg = reply::stake_lp_tokens(
        deps.as_ref(),
        &app,
        config.pool_data.dex,
        AnsAsset::new(lp_token, received_lp),
        config.unbonding_period,
    )?;

    let res = Response::new().add_message(mint_msg).add_message(stake_msg);
    Ok(app.custom_tag_response(
        res,
        "lp_provision_reply",
        vec![("vault_token_minted", mint_amount)],
    ))
}

fn compute_mint_amount(
    staked_lp: Uint128,
    current_vault_supply: Uint128,
    received_lp: Uint128,
) -> Uint128 {
    if !staked_lp.is_zero() {
        // will zero if first deposit
        current_vault_supply
            .checked_multiply_ratio(received_lp, staked_lp)
            .unwrap()
    } else {
        // if first deposit, mint the same amount of tokens as the LP tokens received
        received_lp
    }
}

fn mint_vault_tokens(
    config: &Config,
    user_address: Addr,
    mint_amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    let mint_msg = wasm_execute(config.vault_token.to_string(),
        &Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        },
        vec![],
    )?
    .into();
    Ok(mint_msg)
}
