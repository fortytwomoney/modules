use self::{dex_interface::{DexConfiguration, BoxedDex}, dexes::astroport::AstroportAMM};

// pub mod kujira_dex;
// pub mod kujira_staking;
// pub mod osmosis_staking;
// pub mod osmosis_dex;
// pub mod astroport_staking;
// pub mod astroport_dex;
pub mod dexes;
pub mod dex_interface;
pub mod dex_error;

pub const ASTROPORT: &str = "astroport";
pub const KUJIRA: &str = "kujira";
pub const OSMOSIS: &str = "osmosis";

