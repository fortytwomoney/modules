pub mod abstract_helper;
pub mod vault;

pub type AResult = anyhow::Result<()>;

pub(crate) const USER1: &str = "user1";
pub(crate) const TEST_NAMESPACE: &str = "4t2";
pub(crate) const VAULT_TOKEN: &str = "vault_token";
