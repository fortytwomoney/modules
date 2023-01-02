use abstract_sdk::os::dex::DexName;
use abstract_sdk::os::objects::{AssetEntry, PoolMetadata, PoolReference};
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
    pub dex: DexName,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Assets in the pool
    pub dex_assets: Vec<AssetEntry>,
    /// Pool reference
    pub pool_reference: PoolReference,
    /// Address of the LP token contract
    pub liquidity_token: Addr,
    /// Vault token
    pub vault_token: Addr,
    /// Address that recieves the fee commissions
    pub commission_addr: Addr,
    /// Vault fee structure
    pub fees: FeeConfig,
}

pub const CACHED_USER_ADDR: Item<Addr> = Item::new("cached_user_addr");
pub const CONFIG: Item<Config> = Item::new("config");
