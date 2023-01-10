use std::env::current_dir;
use std::fs::create_dir_all;

use autocompounder::contract::AutocompounderApp;
use cosmwasm_schema::{remove_schemas, write_api};
use forty_two::autocompounder::{
    AutocompounderExecuteMsg, AutocompounderInstantiateMsg, AutocompounderMigrateMsg,
    AutocompounderQueryMsg,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    AutocompounderApp::export_schema(&out_dir);

    write_api! {
        name: "module-schema",
        instantiate: AutocompounderInstantiateMsg,
        query: AutocompounderQueryMsg,
        execute: AutocompounderExecuteMsg,
        migrate: AutocompounderMigrateMsg,
    };
}
