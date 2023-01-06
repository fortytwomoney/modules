use abstract_sdk::os::dex::DexName;
use abstract_sdk::os::objects::{AssetEntry, PoolId, PoolMetadata};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cosmwasm_std::{Addr, Timestamp};
use cw_storage_plus::{Item, Map};
use cw_utils::{Duration, Expiration};

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
    /// Assets in the pool
    pub dex_assets: Vec<AssetEntry>,
    /// Pool address (number or Address)
    pub pool_address: PoolId,
    /// Pool metadata
    pub pool_data: PoolMetadata,
    /// Address of the LP token contract
    pub liquidity_token: Addr,
    /// Vault token
    pub vault_token: Addr,
    /// Address that recieves the fee commissions
    pub commission_addr: Addr,
    /// Vault fee structure
    pub fees: FeeConfig,
    /// Pool bonding period
    pub bonding_period: Option<Duration>,
    /// minimum unbonding cooldown
    pub min_unbonding_cooldown: Option<Duration>,
}

#[cw_serde]
pub struct Claim {
    // timestamp of the start of the unbonding process
    pub unbonding_timestamp: Expiration,
    // amount of vault tokens to be burned
    pub amount_of_vault_tokens_to_burn: Uint128,
    //  amount of lp tokens being unbonded
    pub amount_of_lp_tokens_to_unbond: Uint128,
}

pub const CACHED_USER_ADDR: Item<Addr> = Item::new("cached_user_addr");
pub const LATEST_UNBONDING: Item<Expiration> = Item::new("latest_unbonding");
// Key: User addreess - Value: Amount of vault tokens to be burned
pub const PENDING_CLAIMS: Map<String, Uint128> = Map::new("pending_claims");
// Key: User address - Value: Claim
pub const CLAIMS: Map<String, Vec<Claim>> = Map::new("claims");
pub const CONFIG: Item<Config> = Item::new("config");
