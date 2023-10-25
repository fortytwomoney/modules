use cosmwasm_std::{Env, Addr, StdResult, CosmosMsg, Coin, WasmMsg, to_binary};
use cw_asset::AssetList;



/// Separate native tokens and Cw20's in an `AssetList` and return messages
/// for increasing allowance for the Cw20's.
///
/// ### Returns
/// Returns a `StdResult<(Vec<CosmosMsg>, Vec<Coin>)>` containing the messages
/// for increasing allowance and the native tokens.
pub fn increase_allowance_msgs(
    env: &Env,
    assets: &AssetList,
    recipient: Addr,
) -> StdResult<(Vec<CosmosMsg>, Vec<Coin>)> {
    let (funds, cw20s) = separate_natives_and_cw20s(assets);
    let msgs: Vec<CosmosMsg> = cw20s
        .into_iter()
        .map(|x| {
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: x.address,
                msg: to_binary(&cw20::Cw20ExecuteMsg::IncreaseAllowance {
                    spender: recipient.to_string(),
                    amount: x.amount,
                    expires: Some(cw20::Expiration::AtHeight(env.block.height + 1)),
                })?,
                funds: vec![],
            }))
        })
        .collect::<StdResult<Vec<_>>>()?;
    Ok((msgs, funds))
}

/// Converts an `AssetList` into a `Vec<Coin>` and a `Vec<Cw20Coin>`.
pub fn separate_natives_and_cw20s(assets: &AssetList) -> (Vec<Coin>, Vec<cw20::Cw20Coin>) {
    let mut coins = vec![];
    let mut cw20s = vec![];

    for asset in assets.into_iter() {
        match &asset.info {
            cw_asset::AssetInfo::Native(token) => {
                coins.push(Coin {
                    denom: token.to_string(),
                    amount: asset.amount,
                });
            }
            cw_asset::AssetInfo::Cw20(addr) => {
                cw20s.push(cw20::Cw20Coin {
                    address: addr.to_string(),
                    amount: asset.amount,
                });
            }
            _ => cw_asset::AssetError::InvalidAssetType { ty: asset.info },
        }
    }

    // Cosmos SDK coins need to be sorted and currently wasmd does not sort
    // CosmWasm coins when converting into SDK coins.
    coins.sort_by(|a, b| a.denom.cmp(&b.denom));

    (coins, cw20s)
}