use std::borrow::BorrowMut;

use astroport::generator::{
    Cw20HookMsg, ExecuteMsg as GeneratorExecuteMsg, QueryMsg as GeneratorQueryMsg,
    RewardInfoResponse,
};
use astroport::pair::SimulationResponse;
use cosmwasm_std::{
    to_binary, wasm_execute, Addr, CosmosMsg, Env, QuerierWrapper, StdError, Uint128,
};
use cw20::Cw20Contract;
use cw_asset::{Asset, AssetInfo, AssetList};

use crate::api::dex_interface::{DexInterface, DexQueryResult, DexResult};

#[cosmwasm_schema::cw_serde]
pub struct AstroportConfiguration {
    lp_token_address: Addr,
    staking_contract_address: Addr,
    pair_address: Addr,
    generator_address: Addr,
    pub pool_assets: Vec<AssetInfo>,
    asset_info_a: cw_asset::AssetInfo,
    asset_info_b: cw_asset::AssetInfo,
}

pub struct AstroportAMM<'a> {
    env: &'a Env,
    querier: &'a QuerierWrapper<'a>,
    config: AstroportConfiguration,
}

impl<'a> AstroportAMM<'a>{
    pub fn new(env: &'a Env,  querier: &'a QuerierWrapper, config: AstroportConfiguration) -> Self {
        Self { env, config, querier }
    }

}


impl AstroportAMM<'_> {
    fn liquidity_assets_valid(&self, offer_assets: &Vec<Asset>) -> Result<(), DexError> {
        todo!()
    }

    fn equally_split_liquidity(
        &self,
        offer_assets: &mut Vec<Asset>,
        msgs: &mut Vec<CosmosMsg>,
    ) -> Result<(), DexError> {
        if offer_assets.iter().any(|a| a.amount.is_zero()) {
            // find 0 asset
            let (index, non_zero_offer_asset) = offer_assets
                .iter()
                .enumerate()
                .find(|(_, a)| !a.amount.is_zero())
                .ok_or(DexError::Std(StdError::GenericErr {
                    msg: "not enough assets".to_string(),
                }))?;

            // the other asset in offer_assets is the one with amount zero
            let ask_asset = offer_assets.get((index + 1) % 2).unwrap().info.clone();

            // we want to offer half of the non-zero asset to swap into the ask asset
            let offer_asset = cw_asset::Asset::new(
                non_zero_offer_asset.info.clone(),
                non_zero_offer_asset
                    .amount
                    .checked_div(Uint128::from(2u128))
                    .unwrap(),
            );

            // simulate swap to get the amount of ask asset we can provide after swapping
            let simulated_received = self
                .simulate_swap(offer_asset.clone(), ask_asset.clone(), None)?
                .return_amount;
            let swap_msg = self.swap(offer_asset.clone(), ask_asset.clone(), None, None)?;
            // add swap msg
            msgs.extend(swap_msg);
            // update the offer assets for providing liquidity
            offer_assets.clear();
            offer_assets.extend(
                vec![
                    offer_asset,
                    cw_asset::Asset::new(ask_asset, simulated_received),
                ]
            );
            Ok(())
        } else {
            Ok(())
        }
    }

}

impl DexInterface for AstroportAMM<'_> {
    fn swap(
        &self,
        source_asset: cw_asset::Asset,
        target_asset: cw_asset::AssetInfo,
        belief_price: Option<cosmwasm_std::Decimal>,
        max_spread: Option<cosmwasm_std::Decimal>,
    ) -> DexResult {
        let swap_msg: Vec<CosmosMsg> = match &source_asset.info {
            AssetInfo::Native(_) => vec![wasm_execute(
                self.config.pair_address.to_string(),
                &astroport::pair::ExecuteMsg::Swap {
                    offer_asset: cw_asset_to_astroport(&source_asset)?,
                    ask_asset_info: None,
                    belief_price,
                    max_spread,
                    to: None,
                },
                vec![source_asset.clone().try_into()?],
            )?
            .into()],
            AssetInfo::Cw20(addr) => vec![wasm_execute(
                addr.to_string(),
                &cw20::Cw20ExecuteMsg::Send {
                    contract: self.config.pair_address.to_string(),
                    amount: source_asset.amount,
                    msg: to_binary(&astroport::pair::Cw20HookMsg::Swap {
                        belief_price,
                        ask_asset_info: None,
                        max_spread,
                        to: None,
                    })?,
                },
                vec![],
            )?
            .into()],
            _ => panic!("unsupported asset"),
        };
        Ok(swap_msg)
    }

    fn provide_liquidity(
        &self,
        mut offer_assets: Vec<Asset>,
        max_spread: Option<cosmwasm_std::Decimal>,
    ) -> DexResult {
        let mut msgs = vec![];
        
        self.liquidity_assets_valid(&offer_assets)?;
        self.equally_split_liquidity(&mut offer_assets, &mut msgs)?;


        // approval msgs for cw20 tokens (if present)
        let (appr_msgs, coins) =
            increase_allowance_msgs(&self.env, offer_assets.clone().into(), self.config.pair_address.clone())?;
        msgs.extend(appr_msgs);

        // construct execute msg
        let astroport_assets = offer_assets
            .iter()
            .map(cw_asset_to_astroport)
            .collect::<Result<Vec<_>, _>>()?;

        let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: vec![astroport_assets[0].clone(), astroport_assets[1].clone()],
            slippage_tolerance: None,
            receiver: None,
            auto_stake: None,
        };

        // actual call to pair
        let liquidity_msg = wasm_execute(self.config.pair_address.clone(), &msg, coins)?.into();
        msgs.push(liquidity_msg);

        Ok(msgs)
    }

    fn withdraw_liquidity(&self, amount: Uint128) -> DexResult {
        let hook_msg = astroport::pair::Cw20HookMsg::WithdrawLiquidity { assets: vec![] };

        let withdraw_msg = Asset::cw20(self.config.lp_token_address.clone(), amount) 
            .send_msg(self.config.pair_address.clone(), to_binary(&hook_msg)?)?;

        Ok(vec![withdraw_msg])
    }

    fn stake(&self, amount: Uint128) -> DexResult {
        let cw20_msg = to_binary(&Cw20HookMsg::Deposit {})?;

        let msg: CosmosMsg = wasm_execute(
            self.config.lp_token_address.to_string(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: self.config.generator_address.to_string(),
                amount,
                msg: cw20_msg.clone(),
            },
            vec![],
        )?
        .into();
        Ok(vec![msg])
    }

    fn claim_rewards(&self) -> DexResult {
        let msg: CosmosMsg = wasm_execute(
            self.config.generator_address.to_owned(),
            &GeneratorExecuteMsg::ClaimRewards {
                lp_tokens: vec![self.config.lp_token_address.to_string()],
            },
            vec![],
        )?
        .into();
        Ok(vec![msg])
    }

    fn unstake(&self, amount: Uint128) -> DexResult {
        let msg: CosmosMsg = wasm_execute(
            self.config.generator_address.to_string(),
            &GeneratorExecuteMsg::Withdraw {
                lp_token: self.config.lp_token_address.to_string(),
                amount,
            },
            vec![],
        )?
        .into();
        Ok(vec![msg])
    }

    fn claim(&self) -> DexResult {
        Ok(vec![])
    }

    fn simulate_swap(
            &self,
            offer_asset: cw_asset::Asset,
            target_token: cw_asset::AssetInfo,
            belief_price: Option<cosmwasm_std::Decimal>,
        ) -> DexQueryResult<SimulationResponse> {
             let response: SimulationResponse = self.querier.query(&wasm_smart_query(
            self.config.pair_address.to_string(),
            &astroport::pair::QueryMsg::Simulation {
                offer_asset: cw_asset_to_astroport(&offer_asset)?,
                ask_asset_info: None,
            },
        )?)?;
        // commission paid in result asset
        Ok(response)
        
    }

    // fn query_info(querier: &QuerierWrapper) -> crate::api::dex_interface::DexQueryResult<()> {
    //     todo!()
    // }

    fn query_staked(
        &self,
    ) -> DexQueryResult<cosmwasm_std::Uint128> {
        let stake_balance: Uint128 = self.querier
            .query_wasm_smart(
                self.config.generator_address.clone(),
                &GeneratorQueryMsg::Deposit {
                    lp_token: self.config.lp_token_address.to_string(),
                    user: self.env.contract.address.to_string(),
                },
            )
            .map_err(|e| {
                StdError::generic_err(format!(
                    "Failed to query staked balance for {}. Error: {:?}",
                    self.env.contract.address,
                    e
                ))
            })?;
        Ok(stake_balance)
    }

    fn query_unbonding(
        &self,
    ) -> DexQueryResult<Uint128> {
        // no unbonding for astroport
        Ok(Uint128::zero())
    }

    fn query_rewards(&self) -> DexQueryResult<Vec<cw_asset::AssetInfo>> {
        let reward_info: RewardInfoResponse = self.querier
            .query_wasm_smart(
                self.config.generator_address.clone(),
                &GeneratorQueryMsg::RewardInfo {
                    lp_token: self.config.lp_token_address.to_string(),
                },
            )
            .map_err(|e| {
                StdError::generic_err(format!(
                    "Failed to query reward info on for lp token {}. Error: {:?}",
                    self.config.lp_token_address,
                    e
                ))
            })?;

        let token = match reward_info.base_reward_token {
            astroport::asset::AssetInfo::Token { contract_addr } => AssetInfo::cw20(contract_addr),
            astroport::asset::AssetInfo::NativeToken { denom } => AssetInfo::native(denom),
        };

        let mut tokens = vec![token];

        if let Some(reward_token) = reward_info.proxy_reward_token {
            tokens.push(AssetInfo::cw20(reward_token));
        }
        Ok(tokens)
    }

    fn query_lp_balance(&self) -> DexQueryResult<Uint128> {
        todo!()
    }

    fn query_pool_balances(
        &self,
        owner: Addr,
    ) -> DexQueryResult<Vec<cw_asset::Asset>> {
        todo!()
    }
}

use crate::api::dex_error::DexError;

use super::cw_helpers::{increase_allowance_msgs, wasm_smart_query};
#[cfg(feature = "astroport")]
fn cw_asset_to_astroport(asset: &cw_asset::Asset) -> Result<astroport::asset::Asset, DexError> {
    use cw_asset::AssetError;

    match &asset.info {
        cw_asset::AssetInfoBase::Native(denom) => Ok(astroport::asset::Asset {
            amount: asset.amount,
            info: astroport::asset::AssetInfo::NativeToken {
                denom: denom.clone(),
            },
        }),
        cw_asset::AssetInfoBase::Cw20(contract_addr) => Ok(astroport::asset::Asset {
            amount: asset.amount,
            info: astroport::asset::AssetInfo::Token {
                contract_addr: contract_addr.clone(),
            },
        }),
        _ => Err(AssetError::InvalidAssetType{ ty: asset.info.to_string() }.into()),
    }
}

#[cfg(test)]
pub mod tests {
    use cosmwasm_schema::serde::Deserialize;
    use cosmwasm_std::testing::MockQuerier;
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::to_binary;
    use cosmwasm_std::Coin;

    use cosmwasm_std::coin;
    use cosmwasm_std::from_binary;
    use cosmwasm_std::CosmosMsg;
    use cosmwasm_std::WasmMsg;
    use cw20::Cw20ExecuteMsg;

    use crate::api::dex_interface::DexInterface;

    use super::Astroport;
    use super::AstroportAMM;
    use super::AstroportConfiguration;
    use cosmwasm_std::coins;
    use cosmwasm_std::Decimal;
    use cosmwasm_std::{wasm_execute, Addr};
    use cw_asset::{Asset, AssetInfo};
    use cw_orch::daemon::networks::PHOENIX_1;
    use speculoos::assert_that;
    use std::str::FromStr;

    pub fn create_astro_setup() -> AstroportAMM<'static> {
        return AstroportAMM {

            querier: &MockQuerier::default(),
            env: &mock_env(),
            config: AstroportConfiguration {
                lp_token_address: Addr::unchecked(LP_TOKEN),
                staking_contract_address: Addr::unchecked(STAKING_CONTRACT_ADDRESS),
                pair_address: Addr::unchecked(PAIR_ADDRESS),
                generator_address: Addr::unchecked(GENERATOR_ADDRESS),
                asset_info_a: cw_asset::AssetInfoBase::Native(USDC.to_string()),
                asset_info_b: cw_asset::AssetInfoBase::Native(LUNA.to_string()),
                pool_assets: vec![AssetInfo::native(USDC), AssetInfo::native(LUNA)],
            }
        };
    }

    const PAIR_ADDRESS: &str = "pool-contract";
    const LP_TOKEN: &str = "pair-token";
    const STAKING_CONTRACT_ADDRESS: &str = "staking-contract";
    const GENERATOR_ADDRESS: &str = "generator-contract";
    const USDC: &str = "ibc/B3504E092456BA618CC28AC671A71FB08C6CA0FD0BE7C8A5B5A3E2DD933CC9E4";
    const LUNA: &str = "uluna";

    fn max_spread() -> Decimal {
        Decimal::from_str("0.1").unwrap()
    }

    fn get_wasm_msg<T: for<'de> Deserialize<'de>>(msg: CosmosMsg) -> T {
        match msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => from_binary(&msg).unwrap(),
            _ => panic!("Expected execute wasm msg, got a different enum"),
        }
    }

    fn get_wasm_addr(msg: CosmosMsg) -> String {
        match msg {
            CosmosMsg::Wasm(WasmMsg::Execute { contract_addr, .. }) => contract_addr,
            _ => panic!("Expected execute wasm msg, got a different enum"),
        }
    }

    fn get_wasm_funds(msg: CosmosMsg) -> Vec<Coin> {
        match msg {
            CosmosMsg::Wasm(WasmMsg::Execute { funds, .. }) => funds,
            _ => panic!("Expected execute wasm msg, got a different enum"),
        }
    }

    #[test]
    fn swap() -> anyhow::Result<()> {
        let amount = 100_000u128;
        let amm = create_astro_setup();
        let source_token = Asset::new(AssetInfo::native(USDC), amount);
        let target_token = AssetInfo::native(LUNA);
        let msgs = amm.swap(source_token, target_token, belief_price, Some(max_spread()))?;

        assert_that!(vec![wasm_execute(
            PAIR_ADDRESS,
            &astroport::pair::ExecuteMsg::Swap {
                offer_asset: astroport::asset::Asset {
                    amount: amount.into(),
                    info: astroport::asset::AssetInfo::NativeToken {
                        denom: USDC.to_string(),
                    },
                },
                ask_asset_info: None,
                belief_price: Some(Decimal::from_str("0.2").unwrap()),
                max_spread: Some(max_spread()),
                to: None,
            },
            coins(amount, USDC),
        )
        .unwrap()
        .into()])
        .is_equal_to(msgs);

        Ok(())
    }

    #[test]
    fn provide_liquidity() -> anyhow::Result<()> {
        let amount_usdc = 100_000u128;
        let amount_luna = 50_000u128;

        let msgs = create_astro_setup()
            .provide_liquidity(
                vec![ Asset::native(USDC, amount), Asset::native(LUNA, amount_luna)],
                Some(max_spread()),
            )

            .unwrap();

        assert_that!(vec![wasm_execute(
            PAIR_ADDRESS,
            &astroport::pair::ExecuteMsg::ProvideLiquidity {
                assets: vec![
                    astroport::asset::Asset {
                        amount: amount_usdc.into(),
                        info: astroport::asset::AssetInfo::NativeToken {
                            denom: USDC.to_string(),
                        },
                    },
                    astroport::asset::Asset {
                        amount: amount_luna.into(),
                        info: astroport::asset::AssetInfo::NativeToken {
                            denom: LUNA.to_string(),
                        },
                    },
                ],
                slippage_tolerance: Some(max_spread()),
                auto_stake: Some(false),
                receiver: None,
            },
            vec![coin(amount_usdc, USDC), coin(amount_luna, LUNA)],
        )
        .unwrap()
        .into()])
        .is_equal_to(msgs);
        Ok(())
    }

    #[test]
    fn provide_liquidity_one_side() -> anyhow::Result<()> {
        let amount_usdc = 100_000u128;
        let amount_luna = 0u128;
        let msgs = create_astro_setup().provide_liquidity(
                vec![ Asset::native(USDC, amount), Asset::native(LUNA, amount_luna)],
            Some(max_spread()),
        )?;

        // There should be a swap before providing liquidity
        // We can't really test much further, because this unit test is querying mainnet liquidity pools
        expect_eq(
            wasm_execute(
                PAIR_ADDRESS,
                &astroport::pair::ExecuteMsg::Swap {
                    offer_asset: astroport::asset::Asset {
                        amount: (amount_usdc / 2u128).into(),
                        info: astroport::asset::AssetInfo::NativeToken {
                            denom: USDC.to_string(),
                        },
                    },
                    ask_asset_info: None,
                    belief_price: None,
                    max_spread: Some(max_spread()),
                    to: None,
                },
                coins(amount_usdc / 2u128, USDC),
            )
            .unwrap()
            .into(),
            msgs[0].clone(),
        )
        .unwrap();
        Ok(())
    }

    // #[test]
    // fn provide_liquidity_symmetric() -> anyhow::Result<()> {
    //     let amount_usdc = 100_000u128;
    //     let msgs = create_setup()
    //         .test_provide_liquidity_symmetric(
    //             PoolAddress::contract(Addr::unchecked(PAIR_ADDRESS)),
    //             Asset::new(AssetInfo::native(USDC), amount_usdc),
    //             vec![AssetInfo::native(LUNA)],
    //         )
    //         .unwrap();

    //     assert_eq!(msgs.len(), 1);
    //     assert_eq!(get_wasm_addr(msgs[0].clone()), PAIR_ADDRESS);

    //     let unwrapped_msg: astroport::pair::ExecuteMsg = get_wasm_msg(msgs[0].clone());
    //     match unwrapped_msg {
    //         astroport::pair::ExecuteMsg::ProvideLiquidity {
    //             assets,
    //             slippage_tolerance,
    //             auto_stake,
    //             receiver,
    //         } => {
    //             assert_eq!(assets.len(), 2);
    //             assert_eq!(
    //                 assets[0],
    //                 astroport::asset::Asset {
    //                     amount: amount_usdc.into(),
    //                     info: astroport::asset::AssetInfo::NativeToken {
    //                         denom: USDC.to_string()
    //                     },
    //                 }
    //             );
    //             assert_eq!(slippage_tolerance, None);
    //             assert_eq!(auto_stake, None);
    //             assert_eq!(receiver, None)
    //         }
    //         _ => panic!("Expected a provide liquidity variant"),
    //     }

    //     let funds = get_wasm_funds(msgs[0].clone());
    //     assert_eq!(funds.len(), 2);
    //     assert_eq!(funds[0], coin(amount_usdc, USDC),);

    //     Ok(())
    // }

    #[test]
    fn withdraw_liquidity() {
        let amount_lp = 100_000u128;
        let msgs = create_astro_setup().withdraw_liquidity(amount_lp).unwrap();

        assert_eq!(
            msgs,
            vec![wasm_execute(
                LP_TOKEN,
                &Cw20ExecuteMsg::Send {
                    contract: PAIR_ADDRESS.to_string(),
                    amount: amount_lp.into(),
                    msg: to_binary(&astroport::pair::Cw20HookMsg::WithdrawLiquidity {
                        assets: vec![]
                    })
                    .unwrap()
                },
                vec![]
            )
            .unwrap()
            .into()]
        );
    }

    #[test]
    fn simulate_swap() {
        let amount = 100_000u128;
        // We siply verify it's executed, no check on what is returned
        create_astro_setup()
            .test_simulate_swap(
                PoolAddress::contract(Addr::unchecked(PAIR_ADDRESS)),
                Asset::new(AssetInfo::native(USDC), amount),
                AssetInfo::native(LUNA),
            )
            .unwrap();
    }
}
