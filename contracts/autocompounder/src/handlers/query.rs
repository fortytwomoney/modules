use crate::contract::AutocompounderApp;
use cosmwasm_std::{to_binary,  Binary, Deps, Env, StdResult};
use forty_two::autocompounder::AutocompounderQueryMsg;
use crate::state::{CONFIG, Config};

const _DEFAULT_PAGE_SIZE: u8 = 5;
const _MAX_PAGE_SIZE: u8 = 20;

/// Handle queries sent to this app.
pub fn query_handler(
    deps: Deps,
    _env: Env,
    _app: &AutocompounderApp,
    msg: AutocompounderQueryMsg,
) -> StdResult<Binary> {
    match msg {
        AutocompounderQueryMsg::Config { } => to_binary(&query_config(deps)?)?,
    };
    unimplemented!();
}

/// Returns the current configuration.
pub fn query_config(deps: Deps) -> StdResult<Config> {
    let _config = CONFIG.load(deps.storage)?;

    unimplemented!();
}
