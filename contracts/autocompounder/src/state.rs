use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use cw_utils::Expiration;
pub use crate::msg::{Claim, Config, FeeConfig};

pub const CACHED_USER_ADDR: Item<Addr> = Item::new("cached_user_addr");
pub const CACHED_ASSETS: Map<String, Uint128> = Map::new("cached_assets");
pub const CACHED_FEE_AMOUNT: Item<Uint128> = Item::new("cached_fee_amount");

pub const LATEST_UNBONDING: Item<Expiration> = Item::new("latest_unbonding");
// Key: User addreess - Value: Amount of vault tokens to be burned
pub const PENDING_CLAIMS: Map<String, Uint128> = Map::new("pending_claims");
// Key: User address - Value: Claim
pub const CLAIMS: Map<String, Vec<Claim>> = Map::new("claims");
pub const CONFIG: Item<Config> = Item::new("config");
pub const FEE_CONFIG: Item<FeeConfig> = Item::new("fee_config");
pub const DEFAULT_BATCH_SIZE: u32 = 1000;
pub const MAX_BATCH_SIZE: u32 = 10000;