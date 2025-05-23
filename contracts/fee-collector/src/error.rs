use abstract_app::AppError;
use abstract_core::objects::ans_host::AnsHostError;
use abstract_core::{objects::AssetEntry, AbstractError};
use abstract_sdk::AbstractSdkError;
use cosmwasm_std::{OverflowError, StdError};
use cw_asset::AssetError;
use cw_controllers::AdminError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum FeeCollectorError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Abstract(#[from] AbstractError),

    #[error("{0}")]
    AbstractSdk(#[from] AbstractSdkError),

    #[error("{0}")]
    AnsHostError(#[from] AnsHostError),

    #[error("{0}")]
    Asset(#[from] AssetError),

    #[error("Asset type: {0} is unsupported.")]
    UnsupportedAssetType(String),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    DappError(#[from] AppError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("This contract does not implement the cw20 swap function")]
    NoSwapAvailable {},

    #[error("The provided token: {} is not this vault's LP token", token)]
    NotLPToken { token: String },

    #[error("The asset you wished to remove: {} is not part of the vector", asset)]
    AssetNotPresent { asset: String },

    #[error("The asset you wished to add: {} is already part of the vector", asset)]
    AssetAlreadyPresent { asset: String },

    #[error("The asset {:?} is not whitelisted", asset)]
    AssetNotWhitelisted { asset: AssetEntry },

    #[error("The provided token is not the base token")]
    WrongToken {},

    #[error("The provided native coin is not the same as the claimed deposit")]
    WrongNative {},

    #[error("It's required to use cw20 send message to add liquidity with cw20 tokens")]
    NotUsingCW20Hook {},

    #[error("The provided fee is invalid")]
    InvalidFee {},

    #[error("The deposit asset {0} is not the base asset for holding value calculation")]
    DepositAssetNotBase(String),

    #[error("The actual amount of tokens transfered is different from the claimed amount.")]
    InvalidAmount {},

    #[error("Collect called while no tokens to be swapped")]
    NoTokensAvailableForSwap {},

    #[error("The provided asset is the fee asset and cannot be added to the allowed assets")]
    FeeAssetNotAllowed {},

    #[error("No assets were provided")]
    NoAssetsProvided {},

    #[error(
        "The provided asset {} is not supported by the ANS. original error: {:?}",
        asset,
        error
    )]
    AssetNotKnownByAns { asset: String, error: String },

    #[error("The provided asset {} is not supported by the dex", asset)]
    AssetNotSupportedByDex { asset: String, error: String },
}
