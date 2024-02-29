use crate::contract::INSTANTIATE_REPLY_ID;

use crate::kujira_tx::encode_query_supply_of;
use crate::kujira_tx::max_subdenom_length_for_chain;
use crate::kujira_tx::tokenfactory_burn_msg;
use crate::kujira_tx::tokenfactory_create_denom_msg;
use crate::kujira_tx::tokenfactory_mint_msg;
use crate::kujira_tx::SUPPLY_OF_PATH;
use crate::msg::Config;
use crate::state::CONFIG;
use crate::state::DECIMAL_OFFSET;

use crate::state::VAULT_TOKEN_SYMBOL;
use crate::{
    contract::AutocompounderApp, contract::AutocompounderResult, error::AutocompounderError,
};
use abstract_core::objects::AnsAsset;
use abstract_core::objects::DexAssetPairing;
use abstract_core::objects::PoolMetadata;
use abstract_cw_staking::{msg::*, CW_STAKING_ADAPTER_ID};
use abstract_dex_adapter::DexInterface;
use abstract_sdk::feature_objects::AnsHost;
use abstract_sdk::features::AbstractNameService;
use abstract_sdk::AccountAction;
use abstract_sdk::AdapterInterface;
use abstract_sdk::{core::objects::AssetEntry, features::AccountIdentification};
use abstract_sdk::{AbstractSdkResult, Execution, TransferInterface};
use cosmwasm_std::QueryRequest;

use cosmwasm_std::Coin;
use cosmwasm_std::Reply;
use cosmwasm_std::SupplyResponse;

use cosmwasm_std::{
    to_json_binary, wasm_execute, Addr, CosmosMsg, Decimal, Deps, ReplyOn, StdError, SubMsg,
    Uint128, WasmMsg,
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
/// performs stargate query for the following path: "/cosmos.bank.v1beta1.Query/SupplyOf".
pub fn query_supply_with_stargate(deps: Deps, denom: &str) -> AutocompounderResult<Coin> {
    // this may not work because kujira has its own custom bindings. https://docs.rs/kujira-std/0.8.4/kujira_std/enum.KujiraQuery.html
    let request = QueryRequest::Stargate {
        path: SUPPLY_OF_PATH.to_string(),
        data: encode_query_supply_of(denom).into(),
    };
    let res: SupplyResponse = deps.querier.query(&request)?;
    Ok(res.amount)
}

/// create a SubMsg to instantiate the Vault token with either the tokenfactory(kujira) or a cw20.
pub fn create_vault_token_submsg(
    minter: String,
    subdenom: String,
    code_id: Option<u64>,
    dex: String,
) -> Result<SubMsg, AutocompounderError> {
    if let Some(code_id) = code_id {
        let msg = TokenInstantiateMsg {
            name: subdenom,
            symbol: VAULT_TOKEN_SYMBOL.to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse { minter, cap: None }),
            marketing: None,
        };
        Ok(SubMsg {
            msg: WasmMsg::Instantiate {
                admin: None,
                code_id,
                msg: to_json_binary(&msg)?,
                funds: vec![],
                label: "4T2 Vault Token".to_string(),
            }
            .into(),
            gas_limit: None,
            id: INSTANTIATE_REPLY_ID,
            reply_on: ReplyOn::Success,
        })
    } else {
        let cosmos_msg = tokenfactory_create_denom_msg(minter, subdenom, dex.as_str());
        let sub_msg = SubMsg {
            msg: cosmos_msg,
            gas_limit: None,
            id: 0,
            reply_on: ReplyOn::Never, // this is like sending a normal message
        };

        Ok(sub_msg)
    }
}
// factory/cosmos2contract/V-4T2/wyndex:eur,usd:constant_product

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
    dex: String,
) -> Result<CosmosMsg, AutocompounderError> {
    match config.vault_token.clone() {
        AssetInfo::Native(denom) => {
            tokenfactory_mint_msg(minter, denom, amount, recipient.as_str(), dex.as_str())
                .map_err(|e| e.into())
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
    dex: String,
) -> AutocompounderResult<CosmosMsg> {
    match config.vault_token.clone() {
        AssetInfo::Native(denom) => {
            tokenfactory_burn_msg(minter, denom, amount, dex.as_str()).map_err(|e| e.into())
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
        stakes: vec![lp_token_name],
        staker_address: app.proxy_address(deps)?.to_string(),
        provider: dex,
        unbonding_period,
    };
    let res: StakeResponse = adapters.query(CW_STAKING_ADAPTER_ID, query)?;
    let amount = res
        .amounts
        .first()
        .ok_or(AutocompounderError::Std(StdError::generic_err(
            "No staked assets found",
        )))?;

    Ok(*amount)
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
        CW_STAKING_ADAPTER_ID,
        StakingExecuteMsg {
            provider,
            action: StakingAction::Stake {
                assets: vec![asset],
                unbonding_period,
            },
        },
    )
}

/// creates subdenom that is truncated by the max denom length per app
/// For osmosis: // max length of subdenom for osmosis is 44 https://github.com/osmosis-labs/osmosis/blob/6a53f5611ae27b653a5758333c9a0862835917f4/x/tokenfactory/types/denoms.go#L10-L36
/// For Kujira: // max length of subdenom for kujira is 64 (+8 + 32 ) (they use comsos-sdk validateDenom function https://github.com/Team-Kujira/core/blob/554950147825e94fa52c3ff0a3b138568cf7c774/x/denom/types/denoms.go#L31 https://github.com/cosmos/cosmos-sdk/blob/47770f332c0181924a04c1d87684b8fc62a3bc69/types/coin.go#L833-L841)
pub fn create_subdenom_from_pool_assets(pool_data: &PoolMetadata) -> String {
    let mut full_denom = format!("VT_4T2/{}", pool_data)
        .replace(',', "_")
        .replace('>', "-");
    full_denom.truncate(max_subdenom_length_for_chain(&pool_data.dex));
    full_denom
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
pub fn swap_rewards(
    app: &AutocompounderApp,
    deps: Deps,
    rewards: Vec<AnsAsset>,
) -> Result<Vec<CosmosMsg>, AutocompounderError> {
    let config = CONFIG.load(deps.storage)?;
    let dex_name = config.pool_data.dex;
    let max_spread = config.max_swap_spread;
    let target_assets = config.pool_data.assets;

    let dex = app.ans_dex(deps, dex_name.clone());
    let ans_host = app.ans_host(deps)?;

    let mut swap_msgs = Vec::new();
    for reward in &rewards {
        if !target_assets
            .iter()
            .any(|target_asset| &reward.name == target_asset)
        {
            let target_asset = match_reward_asset_with_pool_asset(
                reward,
                &target_assets,
                &dex_name,
                &ans_host,
                deps,
            )?;

            let swap_msg = dex.swap(reward.clone(), target_asset, Some(max_spread), None)?;
            swap_msgs.push(swap_msg);
        }
    }
    Ok(swap_msgs)
}

fn match_reward_asset_with_pool_asset(
    reward: &AnsAsset,
    target_assets: &[AssetEntry],
    dex_name: &str,
    ans_host: &AnsHost,
    deps: Deps<'_>,
) -> AutocompounderResult<AssetEntry> {
    match check_pair_exists(reward, target_assets[0].clone(), dex_name, ans_host, deps) {
        Ok(()) => Ok(target_assets[0].clone()),
        Err(_) => {
            check_pair_exists(reward, target_assets[1].clone(), dex_name, ans_host, deps)?;
            Ok(target_assets[1].clone())
        }
    }
}

fn check_pair_exists(
    reward: &AnsAsset,
    asset: AssetEntry,
    dex_name: &str,
    ans_host: &AnsHost,
    deps: Deps<'_>,
) -> Result<(), AutocompounderError> {
    let mut assets: [&AssetEntry; 2] = [&reward.name, &asset];
    assets.sort();
    let asset_pairing = DexAssetPairing::new(assets[0].clone(), assets[1].clone(), dex_name);

    ans_host
        .query_asset_pairing(&deps.querier, &asset_pairing)
        .map_err(AutocompounderError::RewardCannotBeSwapped)
        .map(|_| ())
}

pub fn get_last_msgs_with_reply(
    swap_msgs: &mut Vec<CosmosMsg>,
    reply_id: u64,
) -> Result<SubMsg, AutocompounderError> {
    let swap_msg = swap_msgs
        .pop()
        .ok_or(AutocompounderError::Std(StdError::GenericErr {
            msg: "No swap msgs".to_string(),
        }))?;
    let submsg = SubMsg::reply_on_success(swap_msg, reply_id);
    Ok(submsg)
}

pub fn transfer_to_msgs(
    app: &AutocompounderApp,
    deps: Deps,
    asset: AnsAsset,
    recipient: &Addr,
) -> Result<CosmosMsg, AutocompounderError> {
    let actions: Vec<AccountAction> = if asset.amount.is_zero() {
        vec![]
    } else {
        vec![app.bank(deps).transfer(vec![asset], recipient)?]
    };
    Ok(app.executor(deps).execute(actions)?.into())
}

/// computes the minimum cooldown period based on the max claims and unbonding duration.
fn compute_min_unbonding_cooldown(
    max_claims: Option<u32>,
    unbonding_duration: Duration,
) -> Result<Option<Duration>, AutocompounderError> {
    if max_claims.is_none() {
        return Ok(None);
    } else if max_claims == Some(0) {
        return Err(AutocompounderError::Std(StdError::generic_err(
            "Max claims cannot be 0.",
        )));
    }

    let min_unbonding_cooldown = max_claims.map(|max| match &unbonding_duration {
        Duration::Height(block) => Duration::Height(block.saturating_div(max.into())),
        Duration::Time(secs) => Duration::Time(secs.saturating_div(max.into())),
    });
    Ok(min_unbonding_cooldown)
}

pub fn get_unbonding_period_and_cooldown(
    manual_bonding_data: Option<crate::msg::BondingData>,
) -> Result<(Option<Duration>, Option<Duration>), AutocompounderError> {
    let (unbonding_period, min_unbonding_cooldown) = match manual_bonding_data {
        Some(manual_bonding_data) => {
            let unbonding_period = Some(manual_bonding_data.unbonding_period);
            let min_unbonding_cooldown = compute_min_unbonding_cooldown(
                manual_bonding_data.max_claims_per_address,
                manual_bonding_data.unbonding_period,
            )?;
            (unbonding_period, min_unbonding_cooldown)
        }
        None => (None, None),
    };
    Ok((unbonding_period, min_unbonding_cooldown))
}

#[cfg(test)]
pub mod helpers_tests {
    use crate::{
        contract::AUTOCOMPOUNDER_APP,
        kujira_tx::format_tokenfactory_denom,
        test_common::{app_base_mock_querier, app_init, TEST_VAULT_TOKEN},
    };

    use super::*;
    use abstract_core::objects::{pool_id::PoolAddressBase, PoolMetadata};
    use abstract_testing::prelude::{EUR, USD};
    use cosmwasm_std::{
        from_json,
        testing::{mock_dependencies, MockApi, MockStorage},
        Empty, OwnedDeps, Querier, SystemResult,
    };

    use cw_asset::AssetInfoBase;
    use speculoos::{assert_that, result::ResultAssertions};
    use wyndex_bundle::WYND_TOKEN;

    type AResult = anyhow::Result<()>;

    struct MockStargateQuerier {}

    impl MockStargateQuerier {
        fn new() -> Self {
            Self {}
        }
    }

    impl Querier for MockStargateQuerier {
        fn raw_query(&self, bin_request: &[u8]) -> cosmwasm_std::QuerierResult {
            let request: QueryRequest<Empty> = match from_json(bin_request) {
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
                    if path == SUPPLY_OF_PATH {
                        let coin = Coin {
                            denom: "test_vault_token".to_string(),
                            amount: Uint128::from(100u128),
                        };
                        let mut supply = SupplyResponse::default();
                        supply.amount = coin;
                        SystemResult::Ok(cosmwasm_std::ContractResult::Ok(
                            to_json_binary(&supply).unwrap(),
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
            pool_address: PoolAddressBase::Contract(Addr::unchecked("pool_address")),
            pool_data: PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                assets,
            ),
            pool_assets: vec![],
            liquidity_token: AssetInfoBase::Cw20(Addr::unchecked("eur_usd_lp")),
            vault_token: if vt_is_native {
                AssetInfoBase::Native(TEST_VAULT_TOKEN.to_string())
            } else {
                AssetInfoBase::Cw20(Addr::unchecked(TEST_VAULT_TOKEN))
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
        let result = AssetInfo::Native(format_tokenfactory_denom(sender, denom));
        assert_eq!(
            result,
            AssetInfo::Native(format!("factory/{}/{}", sender, denom))
        );
    }

    #[test]
    fn test_create_lp_token_submsg_with_code_id() {
        let minter = "minter".to_string();
        let subdenom = "subdenom".to_string();
        let code_id = Some(1u64);

        let result = create_vault_token_submsg(minter, subdenom, code_id, "kujira".to_string());
        assert_that!(result).is_ok();

        let submsg = result.unwrap();
        assert_that!(submsg.reply_on).is_equal_to(ReplyOn::Success);
        assert_that!(submsg.id).is_equal_to(INSTANTIATE_REPLY_ID);
    }

    #[test]
    fn test_create_lp_token_submsg_without_code_id() {
        let minter = "minter".to_string();

        let result =
            create_vault_token_submsg(minter, "subdenom".to_string(), None, "kujira".to_string());
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
        let dex = "kujira".to_string();

        // native token
        let config = min_cooldown_config(Some(Duration::Time(1)), true);
        let result = mint_vault_tokens_msg(&config, minter, recipient.clone(), amount, dex.clone());
        assert_that!(result.is_ok());
        let msg = result.unwrap();

        let CosmosMsg::Stargate { type_url, value: _ } = msg else {
            panic!("Expected a Stargate message");
        };

        assert_that!(type_url).is_equal_to("/kujira.denom.MsgMint".to_string());

        // cw20 token
        let config = min_cooldown_config(Some(Duration::Time(1)), false);
        let result = mint_vault_tokens_msg(&config, minter, recipient, amount, dex);
        assert_that!(result).is_ok();
        let msg = result.unwrap();
        let CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg: _,
            funds: _,
        }) = msg
        else {
            panic!("Expected a Wasm message");
        };

        assert_that!(contract_addr).is_equal_to("test_vault_token".to_string());
    }
    #[test]
    fn test_mint_burn_tokens_msg() {
        let minter = &Addr::unchecked("minter");
        let amount = Uint128::from(100u128);

        let dex = "kujira".to_string();
        // native token
        let config = min_cooldown_config(Some(Duration::Time(1)), true);
        let result = burn_vault_tokens_msg(&config, minter, amount, dex.clone());
        assert_that!(result.is_ok());
        let msg = result.unwrap();
        let CosmosMsg::Stargate { type_url, value: _ } = msg else {
            panic!("Expected a Stargate message");
        };

        assert_that!(type_url).is_equal_to("/kujira.denom.MsgBurn".to_string());

        // cw20 token
        let config = min_cooldown_config(Some(Duration::Time(1)), false);
        let result = burn_vault_tokens_msg(&config, minter, amount, dex);
        assert_that!(result).is_ok();

        let msg = result.unwrap();
        let CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) = msg
        else {
            panic!("Expected a Wasm message");
        };

        assert_that!(contract_addr).is_equal_to("test_vault_token".to_string());

        assert_that!(from_json(&msg).unwrap())
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

    mod denom {
        use super::*;
        use abstract_core::objects::PoolMetadata;

        fn eur_usd_pool() -> PoolMetadata {
            PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                vec![AssetEntry::new("juno/eur"), AssetEntry::new("juno/usd")],
            )
        }
        fn eur_usd_pool_long_osmosis() -> PoolMetadata {
            PoolMetadata::new(
                "osmosis",
                abstract_core::objects::PoolType::ConstantProduct,
                vec![
                    AssetEntry::new("neutron/eur"),
                    AssetEntry::new("neutron/usd"),
                ],
            )
        }
        fn verylongasset1_verylongasset2_pool_long_kujira() -> PoolMetadata {
            PoolMetadata::new(
                "kujira",
                abstract_core::objects::PoolType::ConstantProduct,
                vec![
                    AssetEntry::new("neutron/verylongasset1"),
                    AssetEntry::new("neutron/verylongasset2"),
                ],
            )
        }

        fn verylongasset1_verylongasset2_pool_long_wyndex() -> PoolMetadata {
            PoolMetadata::new(
                "wyndex",
                abstract_core::objects::PoolType::ConstantProduct,
                vec![
                    AssetEntry::new("neutron/verylongasset1"),
                    AssetEntry::new("neutron/verylongasset2"),
                ],
            )
        }

        #[test]
        fn create_denom_from_pool() {
            let pool = eur_usd_pool();
            let denom = create_subdenom_from_pool_assets(&pool);
            assert_eq!(denom, "VT_4T2/wyndex/juno/eur_juno/usd:constant_product");

            // checks whether the denom is truncated to the max length for any dex
            let long_pool = verylongasset1_verylongasset2_pool_long_wyndex();
            let denom = create_subdenom_from_pool_assets(&long_pool);
            assert_eq!(
                denom,
                "VT_4T2/wyndex/neutron/verylongasset1_neutron/verylongasset2:cons"
            );

            // checks whether the denom is truncated to the max length for osmosis (44)
            let long_pool = eur_usd_pool_long_osmosis();
            let denom = create_subdenom_from_pool_assets(&long_pool);
            assert_eq!(denom, "VT_4T2/osmosis/neutron/eur_neutron/usd:const");

            // checks whether the denom is truncated to the max length for kujira (64)
            let long_pool = verylongasset1_verylongasset2_pool_long_kujira();
            let denom = create_subdenom_from_pool_assets(&long_pool);
            assert_eq!(
                denom,
                "VT_4T2/kujira/neutron/verylongasset1_neutron/verylongasset2:cons"
            );
        }
    }

    mod process_bonding_data {
        /// CAN ONLY TEST THE ERROR CASES, BECAUSE THE SUCCESS CASE WILL TRIGGER A SMART-CONTRACT QUERY/EXECTUTION ON A CW20 CONTRACT
        use super::*;
        use crate::msg::BondingData;

        #[test]
        fn test_get_unbonding_period_and_cooldown_with_manual_bonding_data() {
            let manual_bonding_data = Some(BondingData {
                unbonding_period: Duration::Time(3600),
                max_claims_per_address: Some(6),
            });

            let (unbonding_period, min_unbonding_cooldown) =
                get_unbonding_period_and_cooldown(manual_bonding_data).unwrap();

            assert_eq!(unbonding_period, Some(Duration::Time(3600)));
            assert_eq!(min_unbonding_cooldown, Some(Duration::Time(600)));
        }

        #[test]
        fn test_get_unbonding_period_and_cooldown_without_manual_bonding_data() {
            let manual_bonding_data: Option<BondingData> = None;

            let (unbonding_period, min_unbonding_cooldown) =
                get_unbonding_period_and_cooldown(manual_bonding_data).unwrap();

            assert_eq!(unbonding_period, None);
            assert_eq!(min_unbonding_cooldown, None);
        }
    }

    mod cooldown_tests {
        type AResult = anyhow::Result<()>;

        use super::*;
        #[test]
        fn test_compute_min_unbonding_cooldown_height() -> AResult {
            let max_claims = Some(2);
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, Some(Duration::Height(5)));
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_time() -> AResult {
            let max_claims = Some(2);
            let unbonding_duration = Duration::Time(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, Some(Duration::Time(5)));
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_no_max_claims() -> AResult {
            let max_claims = None;
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration)?;
            assert_eq!(result, None);
            Ok(())
        }

        #[test]
        fn test_compute_min_unbonding_cooldown_zero_max_claims() -> AResult {
            let max_claims = Some(0);
            let unbonding_duration = Duration::Height(10);
            let result = compute_min_unbonding_cooldown(max_claims, unbonding_duration);
            assert_that!(result)
                .is_err()
                .matches(|e| matches!(e, AutocompounderError::Std(_)));
            Ok(())
        }
    }

    #[test]
    fn check_nonexisting_pair_errors() {
        let deps = app_init(false, false);
        let ans_host = AUTOCOMPOUNDER_APP.ans_host(deps.as_ref()).unwrap();
        let dex_name = "wyndex".to_string();

        let result = check_pair_exists(
            &AnsAsset::new(WYND_TOKEN, 1u128),
            AssetEntry::new(EUR),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );
        assert_that!(result)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::RewardCannotBeSwapped(_)));
    }

    #[test]
    fn check_existing_pair_ok() {
        let deps = app_init(false, false);
        let ans_host = AUTOCOMPOUNDER_APP.ans_host(deps.as_ref()).unwrap();
        let dex_name = "wyndex".to_string();

        // normal order
        let result = check_pair_exists(
            &AnsAsset::new(EUR, 1u128),
            AssetEntry::new(USD),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );
        assert_that!(result).is_ok();

        // reverse order
        let result = check_pair_exists(
            &AnsAsset::new(USD, 1u128),
            AssetEntry::new(EUR),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );

        assert_that!(result).is_ok();
    }

    #[test]
    fn test_match_reward_asset_with_pool_asset() {
        let deps = app_init(false, false);
        let ans_host = AUTOCOMPOUNDER_APP.ans_host(deps.as_ref()).unwrap();
        let dex_name = "wyndex".to_string();

        // Set up mock objects
        let target_assets = [AssetEntry::new("eur"), AssetEntry::new("juno")];

        // Test case 1: reward matches first target asset (eur-usd pool exists)
        let reward = AnsAsset::new("usd", 1u128);
        let result = match_reward_asset_with_pool_asset(
            &reward,
            &target_assets.to_vec(),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), target_assets[0].clone());

        // Test case 2: reward matches second target asset
        // (eur-wynd pool does not exist, but the wynd_juno pool exists
        let reward = AnsAsset::new("wynd", 1u128);
        let result = match_reward_asset_with_pool_asset(
            &reward,
            &target_assets.to_vec(),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );
        assert!(result.is_ok());
        assert_that!(result.unwrap()).is_equal_to(target_assets[1].clone());

        // Test case 3: reward matches neither target asset
        let reward = AnsAsset::new("xrp", 1u128);
        let result = match_reward_asset_with_pool_asset(
            &reward,
            &target_assets.to_vec(),
            &dex_name,
            &ans_host,
            deps.as_ref(),
        );
        assert_that!(result)
            .is_err()
            .matches(|e| matches!(e, AutocompounderError::RewardCannotBeSwapped(_)));
    }
}
