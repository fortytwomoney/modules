use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Uint128, SubMsg, Addr, WasmMsg, to_binary, StdError, ReplyOn};
use cw20::MinterResponse;
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;

use forty_two::autocompounder::{AUTOCOMPOUNDER, AutocompounderInstantiateMsg};
use forty_two::autocompounder::{CONFIG, Config, FeeConfig};

use crate::contract::{AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    _app: AutocompounderApp,
    msg: AutocompounderInstantiateMsg,
) -> AutocompounderResult {
    let config: Config = Config {
        fees: FeeConfig {
            performance: msg.performance_fees,
            deposit: msg.deposit_fees,
            withdrawal: msg.withdrawal_fees,
        },
        staking_contract: deps.api.addr_validate(&msg.staking_contract)?,
        liquidity_token: deps.api.addr_validate(&msg.liquidity_token)?,
        commission_addr: deps.api.addr_validate(&msg.commission_addr)?,
    };
    
    let config = Config {
        pool: msg.pool.clone(),
        lp_token: Addr::unchecked(""),
        commission_addr: deps.api.addr_validate(&msg.commission_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;
    FEE_CONFIG.save(deps.storage, &fee_config)?;

    // create LP token SubMsg
    let sub_msg = create_lp_token_submsg(
        env.contract.address.to_string(), 
        msg.pool.clone() + "4T2 Vault Token", 
        "4T2V-".to_string() + &msg.pool, 
        msg.code_id
    )?;



    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}


/// create a SubMsg to instantiate the Vault token.
fn create_lp_token_submsg(minter: String, name: String, symbol: String, code_id: u64) -> Result<SubMsg, StdError> {
    let msg = TokenInstantiateMsg {
        name,
        symbol,
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter,
            cap: None,
        }),
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