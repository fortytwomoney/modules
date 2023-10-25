use cosmwasm_std::StdError;
use thiserror::Error;

use cosmwasm_std::{OverflowError, Uint128};
use cw_asset::AssetError;
use cw_controllers::AdminError;
use cw_utils::Expiration;

#[derive(Error, Debug, PartialEq)]
pub enum AutocompounderError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Admin(#[from] AdminError),

    #[error("{0}")]
    AssetError(#[from] AssetError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("The configured max count has an error, {}", msg)]
    MaxCountError { msg: String },

    #[error("The unbonding periods from the pool are incoherent. They show both block and time durations.")]
    UnbondingPeriodsIncoherent {},

    #[error("Fee cannot exceed 1")]
    InvalidFee {},

    #[error("The asset {asset} is not in the pool of this vault")]
    AssetNotInPool { asset: String },

    #[error("The coin with denom {denom} is not in the pool of this vault")]
    CoinNotInPool { denom: String },

    #[error("The update would exceed the configured max count")]
    ExceededMaxCount {},

    #[error("Withdraw function can only be called by the vault token")]
    SenderIsNotVaultToken {},

    #[error("Deposit can only be called by the lp token")]
    SenderIsNotLpToken {},

    #[error("mismatch of sent {sent} but specified deposit amount of {wanted}")]
    FundsMismatch { sent: Uint128, wanted: Uint128 },

    #[error("Pools with more than 2 assets are not supported")]
    PoolWithMoreThanTwoAssets {},

    #[error("No ongoing claims for address found")]
    NoClaims {},

    #[error("No ongoing claims are ready for withdrawal")]
    NoMaturedClaims {},

    #[error("Minimum cooldown {min_cooldown:?} has not passed since the the latest unbonding {latest_unbonding:?}")]
    UnbondingCooldownNotExpired {
        min_cooldown: cw_utils::Duration,
        latest_unbonding: Expiration,
    },

    #[error("Unbonding is not enabled for this pool")]
    UnbondingNotEnabled {},

    #[error("No rewards to claim")]
    NoRewards {},

    #[error("Zero mint amount is not allowed")]
    ZeroMintAmount {},

    #[error("Zero deposit amount is not allowed")]
    ZeroDepositAmount {},

    #[error("Cannot set recipient to Account")]
    CannotSetRecipientToAccount {},

    #[error("invalid funds. needs to be exactly {wanted_funds}, but was {actual_funds}")]
    InvalidFunds {
        wanted_funds: String,
        actual_funds: String,
    },

    #[error("Vault token is not initialized")]
    VaultTokenNotInitialized {},

    #[error("Vault token is already initialized")]
    VaultTokenAlreadyInitialized {},
}
