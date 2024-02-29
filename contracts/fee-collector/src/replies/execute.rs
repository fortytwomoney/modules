use abstract_sdk::{features::AbstractResponse, Execution, TransferInterface};
use cosmwasm_std::{DepsMut, Env, Reply};

use crate::{
    contract::{FeeCollectorApp, FeeCollectorResult},
    state::CONFIG,
};

pub fn swapped_reply(
    deps: DepsMut,
    _env: Env,
    app: FeeCollectorApp,
    _reply: Reply,
) -> FeeCollectorResult {
    let config = CONFIG.load(deps.storage)?;
    let bank = app.bank(deps.as_ref());

    let fee_balance = bank.balance(&config.fee_asset)?;

    let transfer_msg = bank.transfer(vec![fee_balance], &config.commission_addr)?;

    Ok(app
        .response("swapped_reply")
        .add_message(app.executor(deps.as_ref()).execute(vec![transfer_msg])?))
}
