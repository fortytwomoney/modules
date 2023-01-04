use std::thread::current;

use abstract_sdk::base::features::Identification;

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::ModuleInterface;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult, Uint128,
    WasmMsg,
};
use cw20::{Balance, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;

use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};

use cw20::Cw20QueryMsg::TokenInfo as Cw20TokenInfo;
use protobuf::Message;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::state::{CACHED_USER_ADDR, CONFIG};

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
    _app: AutocompounderApp,
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
    _env: Env,
    app: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let current_vault_supply = vault_token_info.total_supply;

    // 2) Retrieve the number of LP tokens minted/staked.
    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let staked_lp = query_stake(deps.as_ref(), &app, lp_token.clone());
    let cw20::BalanceResponse{
        balance: received_lp,
    } = deps.querier.query_wasm_smart(
        config.vault_token.clone(),
        &cw20::Cw20QueryMsg::Balance {
            address: app.proxy_address(deps.as_ref())?.into_string(),
        },
    )?;

    // The increase in LP tokens held by the vault should be reflected by an equal increase (% wise) in vault tokens. 
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = if !staked_lp.is_zero() {
        // will zero if first deposit
        current_vault_supply.checked_multiply_ratio(received_lp, staked_lp).unwrap()
    } else {
        // if first deposit, mint the same amount of tokens as the LP tokens received
        current_vault_supply + received_lp
    };



    // 4) Mint vault tokens to the user
    let mint_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.vault_token.to_string(),
        msg: to_binary(&Mint {
            recipient: user_address.to_string(),
            amount: mint_amount,
        })?,
        funds: vec![],
    }
    .into();

    // 5) Stake the LP tokens
    let stake_msg = stake_lps(deps, app, "TODO".to_string(), lp_token, received_lp);

    Ok(Response::new()
        .add_message(mint_msg)
        .add_message(stake_msg)
        .add_attribute("vault_token_minted", mint_amount))
}

fn query_stake(deps: Deps, app: &AutocompounderApp, lp_token_name: AssetEntry) -> Uint128 {
    let modules = app.modules(deps);
    let staking_mod = modules.module_address(CW_STAKING).unwrap();

    let query = CwStakingQueryMsg::Stake {
        lp_token_name,
        address: app.proxy_address(deps).unwrap().to_string(),
    };

    let res: StakeResponse = deps.querier.query_wasm_smart(staking_mod, &query).unwrap();
    res.amount

    // TODO: update on new abstract release

    // // // alternative method
    // let modules = app.modules(deps.as_ref());
    // let proxy_stake: StakeResponse = deps.querier.query(
    //     &modules.api_query(CW_STAKING, CwStakingQueryMsg::Stake {
    //         lp_token_name: lp_token_name,               // TODO:: check if this is correct
    //         address: app.proxy_address(deps.as_ref())?.to_string()
    //     })?
    // )?.into();
}

fn stake_lps(
    deps: DepsMut,
    app: AutocompounderApp,
    provider: String,
    lp_token_name: AssetEntry,
    amount: Uint128,
) -> CosmosMsg {
    let modules = app.modules(deps.as_ref());

    let msg: CosmosMsg = modules
        .api_request(
            CW_STAKING,
            CwStakingExecuteMsg {
                provider,
                action: CwStakingAction::Stake {
                    lp_token: AnsAsset::new(lp_token_name, amount),
                },
            },
        )
        .unwrap();

    return msg;
}
