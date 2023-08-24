use crate::contract::INSTANTIATE_REPLY_ID;
use crate::kujira_tx::encode_msg_burn;
use crate::kujira_tx::encode_msg_create_denom;
use crate::kujira_tx::encode_msg_mint;
use crate::kujira_tx::encode_query_supply_of;
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
use cosmwasm_std::QueryRequest;

use cosmwasm_std::Coin;
use cosmwasm_std::Reply;
use cosmwasm_std::SupplyResponse;

use cosmwasm_std::{
    to_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, ReplyOn, StdError, SubMsg, Uint128,
    WasmMsg,
};
use cw20::MinterResponse;
use cw20::{Cw20QueryMsg, TokenInfoResponse};
use cw20_base::msg::ExecuteMsg::Mint;
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;
use cw_asset::AssetError;
use cw_asset::AssetInfo;
use cw_utils::Duration;

use cw_utils::parse_reply_instantiate_data;

// ------------------------------------------------------------
// Helper functions for vault tokens
// ------------------------------------------------------------
/// performs stargate query for the following path: "cosmos.bank.v1beta1.Query/SupplyOf".
pub fn query_supply_with_stargate(deps: Deps, denom: &str) -> AutocompounderResult<Coin> {
    // this may not work because kujira has its own custom bindings. https://docs.rs/kujira-std/latest/kujira_std/enum.KujiraQuery.html
    let request = QueryRequest::Stargate {
        path: "cosmos.bank.v1beta1.Query/SupplyOf".to_string(),
        data: encode_query_supply_of(denom).into(),
    };
    let res: SupplyResponse = deps.querier.query(&request)?;
    Ok(res.amount)
}

/// Formats the native denom to the asset info for the vault token with denom "factory/{`sender`}/{`denom`}"
pub fn format_native_denom_to_asset(sender: &str, denom: &str) -> AssetInfo {
    AssetInfo::Native(format!("factory/{sender}/{denom}"))
}

/// create a SubMsg to instantiate the Vault token with either the tokenfactory(kujira) or a cw20.
pub fn create_vault_token_submsg(
    minter: String,
    name: String,
    symbol: String,
    code_id: Option<u64>,
) -> Result<SubMsg, StdError> {
    if let Some(code_id) = code_id {
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
    } else {
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
    }
}

/// parses the instantiate reply to get the contract address of the vault token or None if kujira. for kujira the denom is already set in instantiate.
pub fn parse_instantiate_reply_cw20(
    reply: Reply,
) -> Result<Option<AssetInfo>, AutocompounderError> {
    let response = parse_reply_instantiate_data(reply)
        .map_err(|err| AutocompounderError::Std(StdError::generic_err(err.to_string())))?;

    let vault_token = AssetInfo::Cw20(Addr::unchecked(response.contract_address));
    Ok(Some(vault_token))
}

/// Creates the message to mint tokens to `recipient`
pub fn mint_vault_tokens_msg(
    config: &Config,
    minter: &Addr,
    recipient: Addr,
    amount: Uint128,
) -> Result<CosmosMsg, AutocompounderError> {
    match config.vault_token.clone() {
        AssetInfo::Native(denom) => {
            let proto_msg = encode_msg_mint(minter.as_str(), denom.as_str(), amount);
            let msg = CosmosMsg::Stargate {
                type_url: "/kujira.denom.MsgMint".to_string(),
                value: to_binary(&proto_msg)?,
            };
            Ok(msg)
        }
        AssetInfo::Cw20(token_addr) => {
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
        _ => Err(AutocompounderError::Std(StdError::generic_err(
            "Vault token is not a cw20 token",
        ))),
    }
}

/// Creates the message to burn tokens from contract
pub fn burn_vault_tokens_msg(
    config: &Config,
    minter: &Addr,
    amount: Uint128,
) -> AutocompounderResult<CosmosMsg> {
    match config.vault_token.clone() {
        AssetInfo::Native(denom) => {
            let proto_msg = encode_msg_burn(minter.as_str(), &denom, amount);
            let msg = CosmosMsg::Stargate {
                type_url: "/kujira.denom.MsgBurn".to_string(),
                value: to_binary(&proto_msg)?,
            };
            Ok(msg)
        }
        AssetInfo::Cw20(token_addr) => {
            let msg = cw20_base::msg::ExecuteMsg::Burn { amount };
            Ok(wasm_execute(token_addr, &msg, vec![])?.into())
        }
        _ => Err(AutocompounderError::AssetError(
            AssetError::InvalidAssetType { ty: "".to_string() },
        )),
    }
}

/// query the total supply of the vault token
pub fn vault_token_total_supply(deps: Deps, config: &Config) -> AutocompounderResult<Uint128> {
    match config.vault_token.clone() {
        AssetInfo::Native(denom) => {
            let supply = query_supply_with_stargate(deps, &denom)?;
            Ok(supply.amount)
        }
        AssetInfo::Cw20(token_addr) => {
            let TokenInfoResponse {
                total_supply: vault_tokens_total_supply,
                ..
            } = deps
                .querier
                .query_wasm_smart(token_addr, &Cw20QueryMsg::TokenInfo {})?;
            Ok(vault_tokens_total_supply)
        }
        _ => Err(AutocompounderError::Std(StdError::generic_err(
            "Vault token is not a cw20 token",
        ))),
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
        .map_err(AutocompounderError::AssetError)
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
pub mod helpers_tests {
    use crate::{contract::AUTOCOMPOUNDER_APP, test_common::app_base_mock_querier};

    use super::*;
    use abstract_core::objects::{pool_id::PoolAddressBase, PoolMetadata};
    use cosmwasm_std::{
        from_binary, from_slice,
        testing::{mock_dependencies, MockApi, MockStorage},
        Empty, OwnedDeps, Querier, SystemResult,
    };
    use cw_asset::AssetInfoBase;
    use speculoos::{assert_that, result::ResultAssertions};

    type AResult = anyhow::Result<()>;

    struct MockStargateQuerier {}

    impl MockStargateQuerier {
        fn new() -> Self {
            Self {}
        }
    }

    impl Querier for MockStargateQuerier {
        fn raw_query(&self, bin_request: &[u8]) -> cosmwasm_std::QuerierResult {
            let request: QueryRequest<Empty> = match from_slice(bin_request) {
                Ok(request) => request,
                Err(err) => {
                    return SystemResult::Err(cosmwasm_std::SystemError::InvalidRequest {
                        error: err.to_string(),
                        request: bin_request.into(),
                    })
                }
            };
            self.handle_query(&request)
        }
    }

    impl MockStargateQuerier {
        fn handle_query(&self, request: &QueryRequest<Empty>) -> cosmwasm_std::QuerierResult {
            match request {
                QueryRequest::Stargate { path, data: _ } => {
                    if path == "cosmos.bank.v1beta1.Query/SupplyOf" {
                        let coin = Coin {
                            denom: "test_vault_token".to_string(),
                            amount: Uint128::from(100u128),
                        };
                        let mut supply = SupplyResponse::default();
                        supply.amount = coin;
                        SystemResult::Ok(cosmwasm_std::ContractResult::Ok(
                            to_binary(&supply).unwrap(),
                        ))
                    } else {
                        SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                            kind: format!("query for path: {path}"),
                        })
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                    kind: "only stargate allowed".to_string(),
                }),
            }
        }
    }

    fn mock_deps_with_stargate() -> OwnedDeps<MockStorage, MockApi, MockStargateQuerier> {
        let custom_querier: MockStargateQuerier = MockStargateQuerier::new();

        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier: custom_querier,
            custom_query_type: Default::default(),
        }
    }

    pub fn min_cooldown_config(
        min_unbonding_cooldown: Option<Duration>,
        vt_is_native: bool,
    ) -> Config {
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
            vault_token: if vt_is_native {
                AssetInfoBase::Native("test_vault_token".to_string())
            } else {
                AssetInfoBase::Cw20(Addr::unchecked("test_vault_token"))
            },
            unbonding_period: Some(Duration::Time(100)),
            min_unbonding_cooldown,
            max_swap_spread: Decimal::percent(50),
        }
    }

    #[test]
    fn test_query_supply_with_stargate() {
        let deps = mock_deps_with_stargate();

        let denom = "test_vault_token";

        let result = query_supply_with_stargate(deps.as_ref(), denom);
        assert!(result.is_ok());
        assert_that!(result).is_ok();
        let supply = result.unwrap();
        assert_that!(supply.amount).is_equal_to(Uint128::from(100u128));
        assert_that!(supply.denom).is_equal_to(denom.to_string());
    }

    #[test]
    fn vault_token_total_supply_native() -> AResult {
        let deps = mock_deps_with_stargate();
        let config = min_cooldown_config(Some(Duration::Time(1)), true);
        let supply = vault_token_total_supply(deps.as_ref(), &config)?;
        assert_that!(supply).is_equal_to(Uint128::from(100u128));
        Ok(())
    }

    #[test]
    fn vault_token_total_supply_cw20() -> AResult {
        let mut deps = mock_dependencies();
        deps.querier = app_base_mock_querier().build();
        let config = min_cooldown_config(Some(Duration::Time(1)), false);
        let supply = vault_token_total_supply(deps.as_ref(), &config)?;
        assert_that!(supply).is_equal_to(Uint128::from(1000u128));
        Ok(())
    }

    #[test]
    fn test_format_native_denom_to_asset() {
        let sender = "sender";
        let denom = "denom";
        let result = format_native_denom_to_asset(sender, denom);
        assert_eq!(
            result,
            AssetInfo::Native(format!("factory/{}/{}", sender, denom))
        );
    }

    #[test]
    fn test_create_lp_token_submsg_with_code_id() {
        let minter = "minter".to_string();
        let name = "name".to_string();
        let symbol = "symbol".to_string();
        let code_id = Some(1u64);
        let result =
            create_vault_token_submsg(minter.clone(), name.clone(), symbol.clone(), code_id);
        assert_that!(result).is_ok();

        let submsg = result.unwrap();
        assert_that!(submsg.reply_on).is_equal_to(ReplyOn::Success);
        assert_that!(submsg.id).is_equal_to(INSTANTIATE_REPLY_ID);
    }

    #[test]
    fn test_create_lp_token_submsg_without_code_id() {
        let minter = "minter".to_string();
        let name = "name".to_string();
        let symbol = "symbol".to_string();
        let result = create_vault_token_submsg(minter.clone(), name.clone(), symbol.clone(), None);
        assert!(result.is_ok());

        let submsg = result.unwrap();
        assert_that!(submsg.reply_on).is_equal_to(ReplyOn::Never);
        assert_that!(submsg.id).is_equal_to(0);
    }

    #[test]
    fn test_mint_vault_tokens_msg() {
        let minter = &Addr::unchecked("minter");
        let recipient = Addr::unchecked("recipient");
        let amount = Uint128::from(100u128);

        // native token
        let config = min_cooldown_config(Some(Duration::Time(1)), true);
        let result = mint_vault_tokens_msg(&config, minter, recipient.clone(), amount);
        assert_that!(result.is_ok());
        let msg = result.unwrap();

        let CosmosMsg::Stargate { type_url, value: _ } = msg else {
            panic!("Expected a Stargate message");
        };

        assert_that!(type_url).is_equal_to("/kujira.denom.MsgMint".to_string());

        // cw20 token
        let config = min_cooldown_config(Some(Duration::Time(1)), false);
        let result = mint_vault_tokens_msg(&config, minter, recipient, amount);
        assert_that!(result).is_ok();
        let msg = result.unwrap();
        let CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg: _, funds: _ }) = msg else {
            panic!("Expected a Wasm message");
        };

        assert_that!(contract_addr).is_equal_to("test_vault_token".to_string());
    }
    #[test]
    fn test_mint_burn_tokens_msg() {
        let minter = &Addr::unchecked("minter");
        let amount = Uint128::from(100u128);

        // native token
        let config = min_cooldown_config(Some(Duration::Time(1)), true);
        let result = burn_vault_tokens_msg(&config, minter, amount);
        assert_that!(result.is_ok());
        let msg = result.unwrap();
        let CosmosMsg::Stargate { type_url, value: _ } = msg else {
            panic!("Expected a Stargate message");
        };

        assert_that!(type_url).is_equal_to("/kujira.denom.MsgBurn".to_string());

        // cw20 token
        let config = min_cooldown_config(Some(Duration::Time(1)), false);
        let result = burn_vault_tokens_msg(&config, minter, amount);
        assert_that!(result).is_ok();

        let msg = result.unwrap();
        let CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, msg, funds: _ }) = msg else {
            panic!("Expected a Wasm message");
        };

        assert_that!(contract_addr).is_equal_to("test_vault_token".to_string());

        assert_that!(from_binary(&msg).unwrap())
            .is_equal_to(cw20_base::msg::ExecuteMsg::Burn { amount });
    }

    #[test]
    fn test_check_fee_valid() -> AResult {
        assert_that!(check_fee(Decimal::percent(0))).is_ok();
        assert_that!(check_fee(Decimal::percent(50))).is_ok();
        assert_that!(check_fee(Decimal::percent(99))).is_ok();
        assert_that!(check_fee(Decimal::percent(100)).is_err());
        Ok(())
    }

    #[test]
    fn test_convert_to_assets() {
        let shares = Uint128::from(100u128);
        let total_assets = Uint128::from(1000u128);
        let total_supply = Uint128::from(500u128);
        let result = convert_to_assets(shares, total_assets, total_supply);
        let reverse = convert_to_shares(result, total_assets, total_supply);
        assert_eq!(result, Uint128::from(200u128 - 4u128)); // rounding error leads to -4
        assert_that!(reverse).is_equal_to(Uint128::from(shares.u128() - 1u128));
        // rounding error leads to -1
    }

    #[test]
    fn test_convert_to_shares() {
        let assets = Uint128::from(100u128);
        let total_assets = Uint128::from(1000u128);
        let total_supply = Uint128::from(500u128);
        let result = convert_to_shares(assets, total_assets, total_supply);
        assert_eq!(result, Uint128::from(50u128));
    }

    #[test]
    fn test_transfer_to_msgs() {
        let deps = mock_dependencies();
        let recipient = Addr::unchecked("recipient");

        // Test transfer with zero amount
        let asset = AnsAsset {
            amount: Uint128::zero(),
            name: AssetEntry::new("token"),
        };
        let msgs = transfer_to_msgs(
            &AUTOCOMPOUNDER_APP,
            deps.as_ref(),
            asset.clone(),
            recipient.clone(),
        )
        .unwrap();
        assert_eq!(msgs.len(), 0);
    }
}
