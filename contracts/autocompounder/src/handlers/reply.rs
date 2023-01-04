use abstract_sdk::base::features::{AbstractNameService, Identification};

use abstract_sdk::os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_sdk::{ModuleInterface, Resolve, TransferInterface};
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Deps, DepsMut, Env, Reply, Response, StdError, StdResult,
    Uint128, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;

use cw_asset::{Asset, AssetInfoBase};
use forty_two::cw_staking::{
    CwStakingAction, CwStakingExecuteMsg, CwStakingQueryMsg, StakeResponse, CW_STAKING,
};

use cw20::Cw20QueryMsg::TokenInfo as Cw20TokenInfo;
use protobuf::Message;

use crate::contract::{AutocompounderApp, AutocompounderResult};
use crate::state::{CACHED_AMOUNT_OF_VAULT_TOKENS_TO_BURN, CACHED_USER_ADDR, CONFIG};

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
    let base_state = app.load_state(deps.storage)?;
    let _proxy = base_state.proxy_address;
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);

    // 1) get the total supply of Vault token
    let vault_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let current_vault_supply = vault_token_info.total_supply;

    // 2) get the number of LP tokens minted in this transaction
    let lp_token_info: TokenInfoResponse = deps
        .querier
        .query_wasm_smart(config.vault_token.clone(), &Cw20TokenInfo {})?;
    let new_lp_token_minted = lp_token_info.total_supply;

    let lp_token = AssetEntry::from(LpToken::from(config.pool_data));
    let vault_stake = query_stake(deps.as_ref(), &app, lp_token.clone()); // TODO: THis might need to change to AssetEntry

    // The total value of all LP tokens that are staked by the proxy are equal to the total value of all vault tokens in circulation
    // 3) Calculate the number of vault tokens to mint
    let mint_amount = new_lp_token_minted
        .checked_multiply_ratio(current_vault_supply, vault_stake)
        .unwrap();

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
    let stake_msg = stake_lps(deps, app, "TODO".to_string(), lp_token, new_lp_token_minted);

    Ok(Response::new()
        .add_message(mint_msg)
        .add_message(stake_msg)
        .add_attribute("vault_token_minted", mint_amount))
}

pub fn lp_withdrawal_reply(
    deps: DepsMut,
    _env: Env,
    dapp: AutocompounderApp,
    _reply: Reply,
) -> AutocompounderResult {
    let config = CONFIG.load(deps.storage)?;
    let base_state = dapp.load_state(deps.storage)?;
    let ans_host = dapp.ans_host(deps.as_ref())?;
    let _proxy = base_state.proxy_address; 
    let user_address = CACHED_USER_ADDR.load(deps.storage)?;
    let amount_of_vault_tokens_to_be_burned =
    CACHED_AMOUNT_OF_VAULT_TOKENS_TO_BURN.load(deps.storage)?;
    CACHED_USER_ADDR.remove(deps.storage);
    CACHED_AMOUNT_OF_VAULT_TOKENS_TO_BURN.remove(deps.storage);
    
    let mut messages = vec![];
    let mut funds: Vec<AnsAsset> = vec![];
    for asset in config.pool_data.assets() {
        let asset_info = asset.resolve(&deps.querier, &ans_host)?;
        
        // HOW TO IDENTIFY WHICH TOKEN AMOUNT CORRESPONDS TO WHICH TOKEN SINCE REPLY EVENTS ONLY CONTAINS TOKEN AMOUNTS BUT NO TOKEN ADDRESS OR DENOM?
        let amount = asset_info.query_balance(&deps.querier, dapp.proxy_address(deps.as_ref())?)?;
        funds.push(AnsAsset::new(asset, amount));
    }
    
    let bank = dapp.bank(deps.as_ref());
    let transfer_msg = bank.transfer(funds, &user_address)?;
    messages.push(transfer_msg);


    let vault_token_burn_msg = get_burn_msg(
        &config.vault_token,
        &user_address,
        amount_of_vault_tokens_to_be_burned,
    )?;
    messages.push(vault_token_burn_msg);

    Ok(Response::new().add_messages(messages))
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

fn get_cw20_transfer_to_msg(
    recipient: &Addr,
    token_addr: &Addr,
    token_amount: Uint128,
) -> StdResult<CosmosMsg> {
    // create transfer cw20 msg
    let transfer_cw20_msg = Cw20ExecuteMsg::Transfer {
        recipient: recipient.into(),
        amount: token_amount,
    };
    let exec_cw20_transfer = WasmMsg::Execute {
        contract_addr: token_addr.into(),
        msg: to_binary(&transfer_cw20_msg)?,
        funds: vec![],
    };
    let cw20_transfer_cosmos_msg: CosmosMsg = exec_cw20_transfer.into();
    Ok(cw20_transfer_cosmos_msg)
}

fn get_bank_transfer_to_msg(recipient: &Addr, denom: &str, native_amount: Uint128) -> CosmosMsg {
    let transfer_bank_msg = cosmwasm_std::BankMsg::Send {
        to_address: recipient.into(),
        amount: vec![Coin {
            denom: denom.to_string(),
            amount: native_amount,
        }],
    };

    let transfer_bank_cosmos_msg: CosmosMsg = transfer_bank_msg.into();
    transfer_bank_cosmos_msg
}

fn get_burn_msg(contract: &Addr, owner: &Addr, amount: Uint128) -> StdResult<CosmosMsg> {
    let msg = cw20_base::msg::ExecuteMsg::BurnFrom {
        owner: owner.to_string(),
        amount,
    };
    Ok(WasmMsg::Execute {
        contract_addr: contract.to_string(),
        msg: to_binary(&msg)?,
        funds: vec![],
    }
    .into())
}
