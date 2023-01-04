use crate::error::StakingError;
use crate::traits::cw_staking::CwStaking;
use crate::traits::identify::Identify;
use cosmwasm_std::{to_binary, Addr, Coin, CosmosMsg, Deps, StdResult, WasmMsg};
use cw20::Cw20ExecuteMsg;
use cw20_junoswap::Denom;
use cw20_stake::msg::{ExecuteMsg as StakeCw20ExecuteMsg, ReceiveMsg};
use cw_asset::{Asset, AssetInfo};

pub const JUNOSWAP: &str = "junoswap";
// Source https://github.com/wasmswap/wasmswap-contracts
pub struct JunoSwap {}

impl Identify for JunoSwap {
    fn over_ibc(&self) -> bool {
        false
    }
    fn name(&self) -> &'static str {
        JUNOSWAP
    }
}

impl CwStaking for JunoSwap {
    fn stake(
        &self,
        _deps: Deps,
        staking_address: Addr,
        asset: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = to_binary(&ReceiveMsg::Stake {})?;
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: asset.info.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: staking_address.into(),
                amount: asset.amount,
                msg,
            })?,
            funds: vec![],
        })])
    }

    fn unstake(
        &self,
        _deps: Deps,
        staking_address: Addr,
        amount: Asset,
    ) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = StakeCw20ExecuteMsg::Unstake {
            amount: amount.amount,
        };
        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }

    fn claim(&self, _deps: Deps, staking_address: Addr) -> Result<Vec<CosmosMsg>, StakingError> {
        let msg = StakeCw20ExecuteMsg::Claim {};

        Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_address.to_string(),
            msg: to_binary(&msg)?,
            funds: vec![],
        })])
    }
}

fn _denom_and_asset_match(denom: &Denom, asset: &AssetInfo) -> Result<bool, StakingError> {
    match denom {
        Denom::Native(denom_name) => match asset {
            cw_asset::AssetInfoBase::Native(asset_name) => Ok(denom_name == asset_name),
            cw_asset::AssetInfoBase::Cw20(_asset_addr) => Ok(false),
            cw_asset::AssetInfoBase::Cw1155(_, _) => Err(StakingError::Cw1155Unsupported),
            _ => panic!("unsupported asset"),
        },
        Denom::Cw20(denom_addr) => match asset {
            cw_asset::AssetInfoBase::Native(_asset_name) => Ok(false),
            cw_asset::AssetInfoBase::Cw20(asset_addr) => Ok(denom_addr == asset_addr),
            cw_asset::AssetInfoBase::Cw1155(_, _) => Err(StakingError::Cw1155Unsupported),
            _ => panic!("unsupported asset"),
        },
    }
}

fn _cw_approve_msgs(assets: &[Asset], spender: &Addr) -> StdResult<Vec<CosmosMsg>> {
    let mut msgs = vec![];
    for asset in assets {
        if let AssetInfo::Cw20(addr) = &asset.info {
            let msg = cw20_junoswap::Cw20ExecuteMsg::IncreaseAllowance {
                spender: spender.to_string(),
                amount: asset.amount,
                expires: None,
            };
            msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: addr.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            }))
        }
    }
    Ok(msgs)
}

fn _coins_in_assets(assets: &[Asset]) -> Vec<Coin> {
    let mut coins = vec![];
    for asset in assets {
        if let AssetInfo::Native(denom) = &asset.info {
            coins.push(Coin::new(asset.amount.u128(), denom.clone()));
        }
    }
    coins
}
