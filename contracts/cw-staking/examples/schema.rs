use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{remove_schemas, write_api};
use cosmwasm_std::Empty;
use cw_staking::contract::CwStakingApi;
use forty_two::cw_staking::{CwStakingQueryMsg, CwStakingExecuteMsg};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    CwStakingApi::export_schema(&out_dir);
}
