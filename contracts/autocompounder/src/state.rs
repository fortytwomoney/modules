use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cw_storage_plus::Item;


#[cw_serde]
pub struct Config {
  pub staking_contract: Addr,
  pub liquidity_token: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
