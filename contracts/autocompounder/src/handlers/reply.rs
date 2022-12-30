use abstract_sdk::base::features::Identification;
use abstract_sdk::os::objects::{AssetEntry, AnsAsset};
use cosmwasm_std::{DepsMut, Env, Reply, Response, StdError, StdResult, Uint128, CosmosMsg, Addr};
use abstract_sdk::ModuleInterface;
use forty_two::cw_staking::{CW_STAKING, CwStakingQueryMsg, StakeResponse, CwStakingExecuteMsg, CwStakingAction};

use cw20::Cw20Contract;
use protobuf::Message;

use crate::contract::{
    AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID, LP_PROVISION_REPLY_ID,
};
use crate::state::{Config, CONFIG};

use crate::response::MsgInstantiateContractResponse;

// pub fn reply_handler(
//     deps: DepsMut,
//     env: Env,
//     app: AutocompounderApp,
//     reply: Reply,
// ) -> AutocompounderResult {
//     // Logic to execute on example reply
//     match reply.id {
//         INSTANTIATE_REPLY_ID => instantiate_reply(deps, env, app, reply),
//         LP_PROVISION_REPLY_ID => lp_provision_reply(deps, env, app, reply),
//     }
// }


/// Handle a relpy for the [`INSTANTIATE_REPLY_ID`] reply.
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
        config.vault_token = Addr::unchecked(vault_token_addr);
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
    let config = CONFIG.load(deps.storage)?;
    let data = reply.result.unwrap().data.unwrap();
    // TODO: What type? Should this be derived from response? Isnt this different for each dex/staking impl?
    //
    //
    //

    let user_address = Addr::unchecked(""); // TODO: Get the user address from the reply

    let lp_token = Cw20Contract(config.liquidity_token);
    let vault_token = Cw20Contract(config.vault_token);

    let base_state = app.load_state(deps.storage)?;

    // 1) get the amount of LP tokens minted and the amount of LP tokens already owned by the proxy
    // LP tokens minted in this transaction
    let new_lp_token_minted = lp_token
        .balance(&deps.querier, base_state.proxy_address.clone())
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
    let stake_msg = stake_lps(deps, app, "TODO".to_string(), config.liquidity_token, new_lp_token_minted);

    // 3) Mint vault tokens to the user
    let mint_msg = vault_token.mint(user_address, mint_amount).unwrap();

    Ok(
        Response::new()
            .add_message(stake_msg)
            .add_message(mint_msg)
            .add_attribute("vault_token_minted", mint_amount)
    )
}

fn query_stake(deps: DepsMut, app: AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    let modules = app.modules(deps.as_ref());
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_address(deps.as_ref()).unwrap().to_string(),
    };
    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
}

fn stake_lps(deps: DepsMut, app: AutocompounderApp, provider: String, lp_token_name: AssetEntry, amount: Uint128) -> CosmosMsg {
    let modules = app.modules(deps.as_ref());

    let msg: CosmosMsg = modules.api_request(CW_STAKING, CwStakingExecuteMsg {
        provider,
        action: CwStakingAction::Stake { lp_token: AnsAsset::new(lp_token_name, amount) }
    }).unwrap();

    return msg
}
