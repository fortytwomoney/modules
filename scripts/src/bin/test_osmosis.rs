use abstract_boot::{
    boot_core::{prelude::*, DaemonOptionsBuilder},
    VersionControl,
};
use abstract_core::objects::{AnsAsset, PoolMetadata};
use clap::Parser;
use cosmwasm_std::Addr;
use forty_two::autocompounder::{
    AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns as AutocompounderQuery, Config,
};
use autocompounder::parse_network;
use autocompounder::vault::Vault;
use log::info;
use speculoos::prelude::*;
use std::sync::Arc;
