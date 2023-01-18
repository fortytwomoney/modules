use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use cw_utils::Expiration;
pub use forty_two::autocompounder::{Config, FeeConfig};

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
