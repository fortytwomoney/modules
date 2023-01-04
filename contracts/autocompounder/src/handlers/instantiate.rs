use abstract_sdk::base::features::AbstractNameService;
use abstract_sdk::os::objects::{ContractEntry, DexAssetPairing, LpToken, PoolReference};
use abstract_sdk::Resolve;
use cosmwasm_std::{
    to_binary, Addr, DepsMut, Env, MessageInfo, ReplyOn, Response, StdError, SubMsg, WasmMsg,
};
use cw20::MinterResponse;
use cw20_base::msg::InstantiateMsg as TokenInstantiateMsg;

use forty_two::autocompounder::{AutocompounderInstantiateMsg, AUTOCOMPOUNDER};

use crate::contract::{AutocompounderApp, AutocompounderResult, INSTANTIATE_REPLY_ID};
use crate::error::AutocompounderError;
use crate::state::{Config, FeeConfig, CONFIG};

/// Initial instantiation of the contract
pub fn instantiate_handler(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    app: AutocompounderApp,
    msg: AutocompounderInstantiateMsg,
) -> AutocompounderResult {
    let ans = app.name_service(deps.as_ref());

    let ans_host = app.ans_host(deps.as_ref())?;

    let AutocompounderInstantiateMsg {
        performance_fees,
        deposit_fees,
        withdrawal_fees,
        commission_addr,
        code_id: _,
        dex,
        pool_assets,
    } = msg;

    if pool_assets.len() > 2 {
        return Err(AutocompounderError::PoolWithMoreThanTwoAssets {});
    }

    // verify that pool assets are valid
    pool_assets.resolve(&deps.querier, &ans_host)?;

    let lp_token = LpToken {
        dex: dex.clone(),
        assets: pool_assets.clone(),
    };
    let lp_token_info = ans.query(&lp_token)?;

    // match on the info and get cw20
    let lp_token_addr: Addr = match lp_token_info {
        cw_asset::AssetInfoBase::Cw20(addr) => Ok(addr),
        _ => Err(AutocompounderError::Std(StdError::generic_err(
            "LP token is not a cw20",
        ))),
    }?;

    let pool_assets_slice = &mut [&pool_assets[0].clone(), &pool_assets[1].clone()];

    let staking_contract_entry = ContractEntry::construct_staking_entry(&dex, pool_assets_slice);
    let staking_contract_addr = ans.query(&staking_contract_entry)?;

    // TODO: Store this in the config
    let pairing = DexAssetPairing::new(
        pool_assets_slice[0].clone(),
        pool_assets_slice[1].clone(),
        &dex,
    );
    let mut pool_references = pairing.resolve(&deps.querier, &ans_host)?;

    assert_eq!(pool_references.len(), 1);
    // Takes the value from the vector
    let pool_reference: PoolReference = pool_references.swap_remove(0);
    // get the pool data
    let pool_data = pool_reference.id.resolve(&deps.querier, &ans_host)?;

    let config: Config = Config {
        fees: FeeConfig {
            performance: performance_fees,
            deposit: deposit_fees,
            withdrawal: withdrawal_fees,
        },
        vault_token: Addr::unchecked(""),
        staking_contract: staking_contract_addr,
        liquidity_token: lp_token_addr,
        commission_addr: deps.api.addr_validate(&commission_addr)?,
        pool_data: pool_data.clone(),
        pool_address: pool_reference.pool_id,
        dex_assets: pool_assets,
        dex: dex.clone(),
    };

    CONFIG.save(deps.storage, &config)?;

    // create LP token SubMsg
    let sub_msg = create_lp_token_submsg(
        env.contract.address.to_string(),
        format!("4T2 Vault Token for {}", pool_data.to_string()),
        "4T2V".to_string(), // TODO: find a better way to define name and symbol
        msg.code_id,
    )?;

    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "instantiate")
        .add_attribute("contract", AUTOCOMPOUNDER))
}

/// create a SubMsg to instantiate the Vault token.
fn create_lp_token_submsg(
    minter: String,
    name: String,
    symbol: String,
    code_id: u64,
) -> Result<SubMsg, StdError> {
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
