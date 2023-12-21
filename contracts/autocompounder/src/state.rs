pub use crate::msg::{Claim, Config, FeeConfig};
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use cw_utils::Expiration;

pub const CACHED_USER_ADDR: Item<Addr> = Item::new("cached_user_addr");
/// Cached contract addresses. Keys are computed by using [`cw_asset::AssetInfo.to_string()`](cw_asset::AssetInfo)
pub const CACHED_ASSETS: Map<String, Uint128> = Map::new("cached_assets");
/// Most recent unbonding call
pub const LATEST_UNBONDING: Item<Expiration> = Item::new("latest_unbonding");
// Key: User addreess - Value: Amount of vault tokens to be burned
pub const PENDING_CLAIMS: Map<Addr, Uint128> = Map::new("pending_claims");
// Key: User address - Value: Claim
pub const CLAIMS: Map<Addr, Vec<Claim>> = Map::new("claims");
pub const CONFIG: Item<Config> = Item::new("config");
pub const FEE_CONFIG: Item<FeeConfig> = Item::new("fee_config");

pub const DEFAULT_BATCH_SIZE: u32 = 100;
pub const MAX_BATCH_SIZE: u32 = 1000;
pub const DECIMAL_OFFSET: u32 = 1;
/// Default max spread for the vault in percentage
pub const DEFAULT_MAX_SPREAD: u32 = 20;
pub const VAULT_TOKEN_SYMBOL: &str = "FTTV";
