use crate::contract::INSTANTIATE_REPLY_ID;
use crate::kujira_tx::encode_msg_create_denom;
use crate::kujira_tx::encode_msg_mint;
use crate::msg::Config;
use crate::state::DECIMAL_OFFSET;

use crate::{
    contract::{AutocompounderApp, AutocompounderResult},
    error::AutocompounderError,
};
use abstract_core::objects::AnsAsset;
use abstract_cw_staking::{msg::*, CW_STAKING};
use abstract_dex_adapter::api::Dex;
use abstract_sdk::AdapterInterface;
use abstract_sdk::{core::objects::AssetEntry, features::AccountIdentification};
use abstract_sdk::{AbstractSdkResult, Execution, TransferInterface};
use cosmwasm_std::Reply;
use cosmwasm_std::{
    to_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, ReplyOn, StdError,
    SubMsg, Uint128, WasmMsg,
};
use cw20::MinterResponse;
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
use cw_asset::AssetInfo;
use cw_utils::Duration;
use cw_utils::parse_reply_instantiate_data;

// ------------------------------------------------------------
// Helper functions for vault tokens
// ------------------------------------------------------------

pub fn format_native_denom_to_asset(sender: &str, denom: &str) -> AssetInfo {
    AssetInfo::Native(
        format!("factory/{sender}/{denom}")
    )
}
/// create a SubMsg to instantiate the Vault token with either the tokenfactory(kujira) or a cw20.
pub fn create_lp_token_submsg(
    minter: String,
    name: String,
    symbol: String,
    code_id: u64,
) -> Result<SubMsg, StdError> {
    if cfg!(feature = "kujira") {
        let msg = encode_msg_create_denom(&minter, &symbol);

        let cosmos_msg = CosmosMsg::Stargate {
            type_url: "/kujira.denom.MsgCreateDenom".to_string(),
            value: to_binary(&msg)?,
        };
        let sub_msg = SubMsg {
            msg: cosmos_msg,
            gas_limit: None,
            id: 0,
            reply_on: ReplyOn::Never, // this is like sending a normal message
        };

        Ok(sub_msg)
    } else {
        let msg = TokenInstantiateMsg {
            name,
            symbol,
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse { minter, cap: None }),
            marketing: None,
        };
        Ok(SubMsg {
            msg: WasmMsg::Instantiate {
                admin: None,
                code_id,
                msg: to_binary(&msg)?,
                funds: vec![],
                label: "4T2 Vault Token".to_string(),
            }
            .into(),
            gas_limit: None,
            id: INSTANTIATE_REPLY_ID,
            reply_on: ReplyOn::Success,
        })
    }
}

/// parses the instantiate reply to get the contract address of the vault token or None if kujira. for kujira the denom is already set in instantiate.
pub fn parse_instantiate_reply(reply: Reply) -> Result<Option<AssetInfo>, AutocompounderError> {
    if cfg!(feature = "kujira") {
        Ok(None)

    } else {
        let response = parse_reply_instantiate_data(reply)
            .map_err(|err| AutocompounderError::Std(StdError::generic_err(err.to_string())))?;
    
        let vault_token = AssetInfo::Cw20(Addr::unchecked(response.contract_address));
        Ok(Some(vault_token))
    }
}


/// Creates the message to mint tokens to `recipient`
pub fn mint_vault_tokens_msg(
    config: &Config,
    minter: &Addr,
    recipient: Addr,
    amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    if cfg!(feature = "kujira") {
        let minter = minter.to_string();
        let AssetInfo::Native(denom) = &config.vault_token else {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Vault token is not a native token",
            )));};

        let proto_msg = encode_msg_mint(&minter, &denom, amount);
        let msg = CosmosMsg::Stargate {
            type_url: "/kujira.denom.MsgMint".to_string(),
            value: to_binary(&proto_msg)?,
        };
        Ok(msg)
    } else {
        let AssetInfo::Cw20(token_addr) = &config.vault_token else {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Vault token is not a cw20 token",
            )));};
        let mint_msg = wasm_execute(
            token_addr.to_string(),
            &Mint {
                recipient: recipient.to_string(),
                amount,
            },
            vec![],
        )?
        .into();
        Ok(mint_msg)
    }
}

/// Creates the message to burn tokens from contract
pub fn burn_vault_tokens_msg(
    config: &Config,
    minter: &Addr,
    amount: Uint128,
) -> AutocompounderResult<CosmosMsg> {
    if cfg!(feature = "kujira") {
        let minter = minter.to_string();
        let AssetInfo::Native(denom) = &config.vault_token else {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Vault token is not a native token",
            )));};

        let proto_msg = encode_msg_mint(&minter, &denom, amount);
        let msg = CosmosMsg::Stargate {
            type_url: "/kujira.denom.MsgBurn".to_string(),
            value: to_binary(&proto_msg)?,
        };
        Ok(msg)
    } else {
        let AssetInfo::Cw20(token_addr) = &config.vault_token else {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Vault token is not a cw20 token",
            )));};
        let msg = cw20_base::msg::ExecuteMsg::Burn { amount };
        Ok(wasm_execute(token_addr, &msg, vec![])?.into())
    }
}

/// query the total supply of the vault token
pub fn vault_token_total_supply(deps: Deps, config: &Config) -> AutocompounderResult<Uint128> {
    if cfg!(feature = "kujira") {
        // raw query using protobuf msg
        // TODO: query the total supply if the token. 

        todo!()
    } else {
        let AssetInfo::Cw20(token_addr) = &config.vault_token else {
            return Err(AutocompounderError::Std(StdError::generic_err(
                "Vault token is not a cw20 token",
            )));};

        let TokenInfoResponse {
            total_supply: vault_tokens_total_supply,
            ..
        } = deps
            .querier
            .query_wasm_smart(token_addr, &Cw20QueryMsg::TokenInfo {})?;
        Ok(vault_tokens_total_supply)
    }
}

/// query the balance of the vault token for user with `addr`
pub fn vault_token_balance(
    deps: Deps,
    config: &Config,
    addr: Addr,
) -> AutocompounderResult<Uint128> {
    config
        .vault_token
        .query_balance(&deps.querier, addr)
        .map_err(|err| AutocompounderError::Std(StdError::generic_err(err.to_string())))
}

// ------------------------------------------------------------
// Other helper functions
// ------------------------------------------------------------

/// queries staking module for the number of staked assets of the app
pub fn query_stake(
    deps: Deps,
    app: &AutocompounderApp,
    dex: String,
    lp_token_name: AssetEntry,
    unbonding_period: Option<Duration>,
) -> AutocompounderResult<Uint128> {
    let adapters = app.adapters(deps);

    let query = StakingQueryMsg::Staked {
        staking_token: lp_token_name,
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
        unbonding_period,
    };
    let res: StakeResponse = adapters.query(CW_STAKING, query)?;
    Ok(res.amount)
}

pub fn stake_lp_tokens(
    deps: Deps,
    app: &AutocompounderApp,
    provider: String,
    asset: AnsAsset,
    unbonding_period: Option<Duration>,
) -> AbstractSdkResult<CosmosMsg> {
    let adapters = app.adapters(deps);
    adapters.request(
        CW_STAKING,
        StakingExecuteMsg {
            provider,
            action: StakingAction::Stake {
                asset,
                unbonding_period,
            },
        },
    )
}

/// Convert vault tokens to lp assets
pub fn convert_to_assets(shares: Uint128, total_assets: Uint128, total_supply: Uint128) -> Uint128 {
    shares.multiply_ratio(
        total_assets + Uint128::from(1u128),
        total_supply + Uint128::from(10u128).pow(DECIMAL_OFFSET),
    )
}

/// Convert lp assets to shares
/// Uses virtual assets to mitigate asset inflation attack. description: https://gist.github.com/Amxx/ec7992a21499b6587979754206a48632
pub fn convert_to_shares(assets: Uint128, total_assets: Uint128, total_supply: Uint128) -> Uint128 {
    assets.multiply_ratio(
        total_supply + Uint128::from(10u128).pow(DECIMAL_OFFSET),
        total_assets + Uint128::from(1u128),
    )
}

pub fn check_fee(fee: Decimal) -> Result<(), AutocompounderError> {
    if fee > Decimal::percent(99) {
        return Err(AutocompounderError::InvalidFee {});
    }
    Ok(())
}
/// swaps all rewards that are not in the target assets and add a reply id to the latest swapmsg
pub fn swap_rewards_with_reply(
    rewards: Vec<AnsAsset>,
    target_assets: Vec<AssetEntry>,
    dex: &Dex<AutocompounderApp>,
    reply_id: u64,
    max_spread: Decimal,
) -> Result<(Vec<CosmosMsg>, SubMsg), AutocompounderError> {
    let mut swap_msgs: Vec<CosmosMsg> = vec![];
    rewards
        .iter()
        .try_for_each(|reward: &AnsAsset| -> AbstractSdkResult<_> {
            if !target_assets.contains(&reward.name) {
                // 3.2) swap to asset in pool
                let swap_msg = dex.swap(
                    reward.clone(),
                    target_assets.get(0).unwrap().clone(),
                    Some(max_spread),
                    None,
                )?;
                swap_msgs.push(swap_msg);
            }
            Ok(())
        })?;
    let swap_msg = swap_msgs.pop().unwrap();
    let submsg = SubMsg::reply_on_success(swap_msg, reply_id);
    Ok((swap_msgs, submsg))
}

pub fn transfer_to_msgs(
    app: &AutocompounderApp,
    deps: Deps,
    asset: AnsAsset,
    recipient: Addr,
) -> Result<Vec<CosmosMsg>, AutocompounderError> {
    if asset.amount.is_zero() {
        Ok(vec![])
    } else {
        Ok(vec![app.executor(deps).execute(vec![app
            .bank(deps)
            .transfer(vec![asset], &recipient)?])?])
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use abstract_core::objects::{pool_id::PoolAddressBase, PoolMetadata};
    use cw_asset::AssetInfoBase;

    pub fn min_cooldown_config(min_unbonding_cooldown: Option<Duration>) -> Config {
        let assets = vec![AssetEntry::new("eur"), AssetEntry::new("usd")];

        Config {
            staking_target: abstract_cw_staking::msg::StakingTarget::Contract(Addr::unchecked(
                "staking_addr",
            )),
            pool_address: PoolAddressBase::Contract(Addr::unchecked("pool_address")),
            pool_data: PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                assets,
            ),
            pool_assets: vec![],
            liquidity_token: AssetInfoBase::Cw20(Addr::unchecked("eur_usd_lp")),
            vault_token: AssetInfoBase::Cw20(Addr::unchecked("test_vault_token")),
            unbonding_period: Some(Duration::Time(100)),
            min_unbonding_cooldown,
            max_swap_spread: Decimal::percent(50),
        }
    }
}
