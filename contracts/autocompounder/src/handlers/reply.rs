use abstract_sdk::os::objects::AssetEntry;
use cosmwasm_std::{DepsMut, Env, Reply, Response, StdError, StdResult, Uint128};
use abstract_sdk::ModuleInterface;
use forty_two::cw_staking::{CW_STAKING, CwStakingQueryMsg, StakeResponse};

use cw20::Cw20Contract;
use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID, LP_PROVISION_REPLY_ID,
};
use crate::state::{Config, CONFIG};

use crate::response::MsgInstantiateContractResponse;

pub fn reply_handler(
    deps: DepsMut,
    env: Env,
    app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    match reply.id {
        INSTANTIATE_REPLY_ID => instantiate_reply(deps, env, app, reply),
        LP_PROVISION_REPLY_ID => lp_provision_reply(deps, env, app, reply),
    }
}

pub fn instantiate_reply(
    deps: DepsMut,
    _env: Env,
    app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    let vault_token_addr = res.get_contract_address();

    CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
        config.lp_token = vault_token_addr.parse()?;
        Ok(config)
    })?;

    Ok(Response::new().add_attribute("vault_token_addr", vault_token_addr))
}

pub fn lp_provision_reply(
    deps: DepsMut,
    env: Env,
    app: AutocompounderApp,
    reply: Reply,
) -> AutocompounderResult {
    // Logic to execute on example reply
    let data = reply.result.unwrap().data.unwrap();

    // 1) get the amount of LP tokens minted and the amount of LP tokens already owned by the proxy
    // LP tokens minted in this transaction
    let new_lp_token_minted: Uint128;

    let config = CONFIG.load(deps.storage)?;
    let lp_token = Cw20Contract(config.liquidity_token);
    let vault_token = Cw20Contract(config.vault_token);

    new_lp_token_minted = lp_token
        .balance(deps.api, app.proxy_addr.clone())
        .unwrap();

    // LP tokens currently owned by the proxy (Assuming all owned LP tokens are staked)
    let vault_stake = query_stake(deps, app, config.liquidity_token); // TODO: THis might need to change to AssetEntry

    // Current amount of vault tokens in circulation
    let current_vault_supply = vault_token.meta(&deps.querier).unwrap().total_supply;

    // The total value of all LP tokens that are staked by the proxy are equal to the total value of all vault tokens in circulation
    // mint_amount =  (current_vault_amount / lp_token_minted) * new_lp_tokens_minted]}
    let mint_amount = new_lp_token_minted.checked_multiply_ratio(
        current_vault_supply, vault_stake).unwrap();
    
    // 2) Stake the LP tokens
    // TODO: This is where we would stake the LP tokens

    // 3) Mint vault tokens to the user
    // TODO: This is where we would mint vault tokens to the user



    Ok(Response::new().add_attribute("vault_token_minted", mint_amount))
}

fn query_stake(deps: DepsMut, app: AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    let modules = app.modules(deps.as_ref());
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_addr.clone(),
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();


}