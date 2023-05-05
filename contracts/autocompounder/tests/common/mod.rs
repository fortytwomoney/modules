pub mod abstract_helper;
pub mod vault;

pub type AResult = anyhow::Result<()>;

pub(crate) const OWNER: &str = "owner";
pub(crate) const USER1: &str = "user1";
