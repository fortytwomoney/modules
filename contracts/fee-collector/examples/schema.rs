use cosmwasm_schema::remove_schemas;
#[cfg(feature = "schema")]
use fee_collector::contract::FeeCollectorApp;
use std::env::current_dir;
use std::fs::create_dir_all;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    #[cfg(feature = "schema")]
    FeeCollectorApp::export_schema(&out_dir);
}
