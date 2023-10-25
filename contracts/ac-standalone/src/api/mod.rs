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

pub fn create_dex_from_config(config: DexConfiguration) -> BoxedDex {
    match config {
        DexConfiguration::Astroport(astroport_config) => Box::new(AstroportAMM::from(astroport_config)),
        DexConfiguration::Osmosis(osmosis_config) => panic!("Osmosis not supported yet"),
        DexConfiguration::Kujira(kujira_config) => panic!("Kujira not supported yet"), 
    }
}