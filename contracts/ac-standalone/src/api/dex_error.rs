use cosmwasm_std::{OverflowError, StdError};
use cw_asset::AssetError;
use cw_controllers::AdminError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]

pub enum DexError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    AssetError(#[from] AssetError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("unknown error: {0}")]
    UnknownError(String),
    
}