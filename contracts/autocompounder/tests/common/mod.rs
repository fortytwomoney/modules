pub mod abstract_helper;
pub mod vault;
pub mod dexes;
pub mod osmosis_pool_incentives_module;
pub mod account_setup;
pub mod integration;

pub type AResult = anyhow::Result<()>;

pub(crate) const USER1: &str = "user1";
pub(crate) const OWNER: &str = "owner";
pub(crate) const TEST_NAMESPACE: &str = "4t2";
pub(crate) const VAULT_TOKEN: &str = "vault_token";
pub(crate) const COMMISSION_RECEIVER: &str = "commission_receiver";
