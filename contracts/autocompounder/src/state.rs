use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cw_storage_plus::Item;

#[cw_serde]
pub struct FeeConfig {
    pub performance: Uint128,
    pub deposit: Uint128,
    pub withdrawal: Uint128,
}

#[cw_serde]
pub struct Config {
    /// Address of the staking contract
    pub staking_contract: Addr,
    /// Address of the LP token contract
    pub liquidity_token: Addr,
    /// Vault fee structure
    pub fees: FeeConfig,
}

pub const CONFIG: Item<Config> = Item::new("config");
