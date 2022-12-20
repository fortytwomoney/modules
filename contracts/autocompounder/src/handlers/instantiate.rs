use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Uint128};

use forty_two::autocompounder::{AUTOCOMPOUNDER, AutocompounderInstantiateMsg};
use crate::state::{ CONFIG, Config, FeeConfig};

use crate::contract::{AutocompounderApp, AutocompounderResult};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    _env: Env,
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
    };

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}
