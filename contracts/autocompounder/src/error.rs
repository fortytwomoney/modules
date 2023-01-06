use abstract_app::AppError;
use cosmwasm_std::{OverflowError, StdError, Uint128};
use cw_controllers::AdminError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum AutocompounderError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    DappError(#[from] AppError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("The configured max count has an error, {}", msg)]
    MaxCountError { msg: String },

    #[error("The update would exceed the configured max count")]
    ExceededMaxCount {},

    #[error("Withdraw function can only be called by the liquidity token")]
    SenderIsNotVaultToken {},

    #[error("mismatch of sent {sent} but specified deposit amount of {wanted}")]
    FundsMismatch { sent: Uint128, wanted: Uint128 },

    #[error("Pools with more than 2 assets are not supported")]
    PoolWithMoreThanTwoAssets {},

    #[error("No ongoing claims are ready for withdrawal")]
    NoMaturedClaims {},
}
