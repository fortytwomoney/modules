#[cfg(test)]
mod test_utils;

use abstract_boot::{Abstract, ManagerQueryFns};

use abstract_os::api::BaseExecuteMsgFns;

use abstract_os::objects::{AnsAsset, AssetEntry, LpToken};
use abstract_os::EXCHANGE;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::{
    factory::{
        ExecuteMsg as FactoryExecuteMsg, InstantiateMsg as FactoryInstantiateMsg, PairConfig,
        PairType, QueryMsg as FactoryQueryMsg,
    },
    generator::{
        Cw20HookMsg as GeneratorHookMsg, ExecuteMsg as GeneratorExecuteMsg,
        InstantiateMsg as GeneratorInstantiateMsg, PendingTokenResponse,
        QueryMsg as GeneratorQueryMsg,
    },
    generator_proxy::InstantiateMsg as ProxyInstantiateMsg,
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg as VestingHookMsg, InstantiateMsg as VestingInstantiateMsg, VestingAccount,
        VestingSchedule, VestingSchedulePoint,
    },
};
use boot_core::deploy::Deploy;
use boot_core::{prelude::*, TxHandler};

use boot_cw_plus::Cw20;
use cosmwasm_std::{to_binary, Addr, Binary, Decimal, Empty, StdResult, Uint128, Uint64};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_multi_test::{App, ContractWrapper, Executor};
use forty_two::autocompounder::{AutocompounderExecuteMsgFns, AutocompounderQueryMsgFns};
use forty_two::autocompounder::{Cw20HookMsg, AUTOCOMPOUNDER};
use forty_two::cw_staking::CW_STAKING;

use speculoos::assert_that;
use test_utils::abstract_helper::{self, init_auto_compounder};
use test_utils::astroport::{Astroport, PoolWithProxy, EUR_TOKEN, USD_TOKEN};
use test_utils::vault::Vault;
use test_utils::OWNER;

const ASTROPORT: &str = "astroport";
const COMMISSION_RECEIVER: &str = "commission_receiver";
const VAULT_TOKEN: &str = "vault_token";

fn create_vault(mock: Mock) -> Result<Vault<Mock>, BootError> {
    let version = "1.0.0".parse().unwrap();
    // Deploy abstract
    let abstract_ = Abstract::deploy_on(mock.clone(), version)?;
    // Deploy Astroport
    let astroport = Astroport::deploy_on(mock.clone(), Empty {})?;

    let eur_asset = AssetEntry::new(EUR_TOKEN);
    let usd_asset = AssetEntry::new(USD_TOKEN);

    // Set up the dex and staking contracts
    let exchange_api = abstract_helper::init_exchange(mock.clone(), &abstract_, None)?;
    let staking_api = abstract_helper::init_staking(mock.clone(), &abstract_, None)?;
    let auto_compounder = init_auto_compounder(mock.clone(), &abstract_, None)?;

    let mut vault_token = Cw20::new(VAULT_TOKEN, mock.clone());
    // upload the vault token code
    let vault_toke_code_id = vault_token.upload()?.uploaded_code_id()?;
    // Create an OS that we will turn into a vault
    let os = abstract_.os_factory.create_default_os(
        abstract_os::objects::gov_type::GovernanceDetails::Monarchy {
            monarch: mock.sender.to_string(),
        },
    )?;
    // install dex
    os.manager.install_module(EXCHANGE, &Empty {})?;
    // install staking
    os.manager.install_module(CW_STAKING, &Empty {})?;
    // install autocompounder
    os.manager.install_module(
        AUTOCOMPOUNDER,
        &abstract_os::app::InstantiateMsg {
            app: forty_two::autocompounder::AutocompounderInstantiateMsg {
                code_id: vault_toke_code_id,
                commission_addr: COMMISSION_RECEIVER.to_string(),
                deposit_fees: Decimal::percent(3),
                dex: ASTROPORT.to_string(),
                fee_asset: eur_asset.to_string(),
                performance_fees: Decimal::percent(3),
                pool_assets: vec![eur_asset.clone(), usd_asset.clone()],
                withdrawal_fees: Decimal::percent(3),
            },
            base: abstract_os::app::BaseInstantiateMsg {
                ans_host_address: abstract_.ans_host.addr_str()?,
            },
        },
    )?;
    // get its address
    let auto_compounder_addr = os
        .manager
        .module_addresses(vec![AUTOCOMPOUNDER.into()])?
        .modules[0]
        .1
        .clone();
    // set the address on the contract
    auto_compounder.set_address(&Addr::unchecked(auto_compounder_addr.clone()));

    // give the autocompounder permissions to call on the dex and cw-staking contracts
    exchange_api
        .call_as(&os.manager.address()?)
        .update_traders(vec![auto_compounder_addr.clone()], vec![])?;
    staking_api
        .call_as(&os.manager.address()?)
        .update_traders(vec![auto_compounder_addr.clone()], vec![])?;

    // set the vault token address
    let auto_compounder_config = auto_compounder.config()?;
    vault_token.set_address(&auto_compounder_config.vault_token);

    Ok(Vault {
        os,
        auto_compounder,
        vault_token,
        abstract_os: abstract_,
        astroport,
        dex: exchange_api,
        staking: staking_api,
    })
}

#[test]
fn generator_without_reward_proxies() -> Result<(), BootError> {
    let owner = Addr::unchecked(test_utils::OWNER);
    // create testing environment
    let (_state, mock) = instantiate_default_mock_env(&owner)?;

    // create a vault
    let vault = crate::create_vault(mock.clone())?;
    let Astroport {
        eur_token,
        usd_token,
        eur_usd_lp,
        generator,
        ..
    } = vault.astroport;
    let vault_token = vault.vault_token;
    let auto_compounder_addr = vault.auto_compounder.addr_str()?;
    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");

    // # deposit into the auto-compounder #

    // give user some funds
    eur_token.mint(&owner, 100_000u128)?;
    usd_token.mint(&owner, 100_000u128)?;

    // increase allowance
    eur_token.increase_allowance(&auto_compounder_addr, 10_000u128, None)?;
    usd_token.increase_allowance(&auto_compounder_addr, 10_000u128, None)?;

    // initial deposit must be > 1000 (of both assets)
    // this is set by Astroport
    vault.auto_compounder.deposit(vec![
        AnsAsset::new(eur_asset.clone(), 10000u64),
        AnsAsset::new(usd_asset, 10000u64),
    ])?;

    // single asset deposit
    eur_token.increase_allowance(&auto_compounder_addr, 1_000u128, None)?;
    vault
        .auto_compounder
        .deposit(vec![AnsAsset::new(eur_asset.clone(), 1000u64)])?;

    // check that the vault token is minted
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(9004u128);
    // and eur balance decreased
    let eur_balance = eur_token.balance(&owner)?;
    assert_that!(eur_balance).is_equal_to(89_000u128);

    // withdraw part from the auto-compounder
    vault_token.send(&Cw20HookMsg::Redeem {}, 3004, auto_compounder_addr.clone())?;
    // check that the vault token decreased
    let vault_token_balance = vault_token.balance(&owner)?;
    assert_that!(vault_token_balance).is_equal_to(6000u128);
    // and eur balance increased
    let eur_balance = eur_token.balance(&owner)?;
    // assert_that!(eur_balance).is_equal_to(90_000u128);

    mock.next_block()?;

    // mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user1, 10);

    // let msg = Cw20ExecuteMsg::Send {
    //     contract: generator_instance.to_string(),
    //     msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
    //     amount: Uint128::new(10),
    // };

    // assert_eq!(
    //     app.execute_contract(user1.clone(), lp_cny_eur.clone(), &msg, &[])
    //         .unwrap_err()
    //         .root_cause()
    //         .to_string(),
    //     "Cannot Sub with 9 and 10".to_string()
    // );

    // mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 1);

    // deposit_lp_tokens_to_generator(
    //     &mut app,
    //     &generator_instance,
    //     USER1,
    //     &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    // );

    // check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 10);
    // check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);

    let generator_staked_balance = eur_usd_lp.balance(&generator)?;
    assert_that!(generator_staked_balance).is_equal_to(9004u128);
    Ok(())

    // check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER1, (0, None));
    // check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER1, (0, None));

    // // User can't withdraw if they didn't deposit
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_cny_eur.to_string(),
    //     amount: Uint128::new(1_000000),
    // };
    // assert_eq!(
    //     app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
    //         .unwrap_err()
    //         .root_cause()
    //         .to_string(),
    //     "Insufficient balance in contract to process claim".to_string()
    // );

    // // User can't emergency withdraw if they didn't deposit
    // let msg = GeneratorExecuteMsg::EmergencyWithdraw {
    //     lp_token: lp_cny_eur.to_string(),
    // };
    // assert_eq!(
    //     app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
    //         .unwrap_err()
    //         .root_cause()
    //         .to_string(),
    //     "astroport::generator::UserInfo not found".to_string()
    // );

    // app.update_block(|bi| next_block(bi));

    // // 10 tokens per block split equally between 2 pools
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER1,
    //     (5_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER1,
    //     (5_000000, None),
    // );

    // // User 2
    // mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user2, 10);
    // mint_tokens(&mut app, pair_eur_usd.clone(), &lp_eur_usd, &user2, 10);

    // deposit_lp_tokens_to_generator(
    //     &mut app,
    //     &generator_instance,
    //     USER2,
    //     &[(&lp_cny_eur, 10), (&lp_eur_usd, 10)],
    // );

    // check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 20);
    // check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 20);

    // // 10 tokens have been distributed to depositors since the last deposit
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER1,
    //     (5_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER1,
    //     (5_000000, None),
    // );

    // // New deposits can't receive already calculated rewards
    // check_pending_rewards(&mut app, &generator_instance, &lp_cny_eur, USER2, (0, None));
    // check_pending_rewards(&mut app, &generator_instance, &lp_eur_usd, USER2, (0, None));

    // // Change pool alloc points
    // let msg = GeneratorExecuteMsg::SetupPools {
    //     pools: vec![
    //         (lp_cny_eur.to_string(), Uint128::from(60u32)),
    //         (lp_eur_usd.to_string(), Uint128::from(40u32)),
    //     ],
    // };
    // app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // app.update_block(|bi| next_block(bi));

    // // 60 to cny_eur, 40 to eur_usd. Each is divided for two users
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER1,
    //     (8_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER1,
    //     (7_000000, None),
    // );

    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER2,
    //     (3_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER2,
    //     (2_000000, None),
    // );

    // // User1 emergency withdraws and loses already accrued rewards (5).
    // // Pending tokens (3) will be redistributed to other staked users.
    // let msg = GeneratorExecuteMsg::EmergencyWithdraw {
    //     lp_token: lp_cny_eur.to_string(),
    // };
    // app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER1,
    //     (0_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER1,
    //     (7_000000, None),
    // );

    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_cny_eur,
    //     USER2,
    //     (3_000000, None),
    // );
    // check_pending_rewards(
    //     &mut app,
    //     &generator_instance,
    //     &lp_eur_usd,
    //     USER2,
    //     (2_000000, None),
    // );

    // // Balance of the generator should be decreased
    // check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 10);

    // // User1 can't withdraw after emergency withdraw
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_cny_eur.to_string(),
    //     amount: Uint128::new(1_000000),
    // };
    // assert_eq!(
    //     app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
    //         .unwrap_err()
    //         .root_cause()
    //         .to_string(),
    //     "Insufficient balance in contract to process claim".to_string(),
    // );

    // // User2 withdraw and get rewards
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_cny_eur.to_string(),
    //     amount: Uint128::new(10),
    // };
    // app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // check_token_balance(&mut app, &lp_cny_eur, &generator_instance, 0);
    // check_token_balance(&mut app, &lp_cny_eur, &user1, 10);
    // check_token_balance(&mut app, &lp_cny_eur, &user2, 10);

    // check_token_balance(&mut app, &astro_token_instance, &user1, 0);
    // check_token_balance(&mut app, &astro_token_instance, &user2, 3_000000);
    // // 7 + 2 distributed ASTRO (for other pools). 5 orphaned by emergency withdrawals, 6 transfered to User2

    // // User1 withdraws and gets rewards
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_eur_usd.to_string(),
    //     amount: Uint128::new(5),
    // };
    // app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 15);
    // check_token_balance(&mut app, &lp_eur_usd, &user1, 5);

    // check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // // User1 withdraws and gets rewards
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_eur_usd.to_string(),
    //     amount: Uint128::new(5),
    // };
    // app.execute_contract(user1.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 10);
    // check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    // check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);

    // // User2 withdraws and gets rewards
    // let msg = GeneratorExecuteMsg::Withdraw {
    //     lp_token: lp_eur_usd.to_string(),
    //     amount: Uint128::new(10),
    // };
    // app.execute_contract(user2.clone(), generator_instance.clone(), &msg, &[])
    //     .unwrap();

    // check_token_balance(&mut app, &lp_eur_usd, &generator_instance, 0);
    // check_token_balance(&mut app, &lp_eur_usd, &user1, 10);
    // check_token_balance(&mut app, &lp_eur_usd, &user2, 10);

    // check_token_balance(&mut app, &astro_token_instance, &user1, 7_000000);
    // check_token_balance(&mut app, &astro_token_instance, &user2, 5_000000);
}

fn mock_app() -> App {
    App::default()
}

fn store_token_code(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn store_factory_code(app: &mut App) -> u64 {
    let factory_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    );

    app.store_code(factory_contract)
}

fn store_pair_code_id(app: &mut App) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );

    app.store_code(pair_contract)
}

fn store_pair_stable_code_id(app: &mut App) -> u64 {
    let pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_stable::contract::execute,
            astroport_pair_stable::contract::instantiate,
            astroport_pair_stable::contract::query,
        )
        .with_reply_empty(astroport_pair_stable::contract::reply),
    );

    app.store_code(pair_contract)
}

fn instantiate_token(app: &mut App, token_code_id: u64, name: &str, cap: Option<u128>) -> Addr {
    let name = String::from(name);

    let msg = TokenInstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw_astro::MinterResponse {
            minter: String::from(OWNER),
            cap: cap.map(Uint128::from),
        }),
        marketing: None,
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

fn instantiate_factory(
    app: &mut App,
    factory_code_id: u64,
    token_code_id: u64,
    pair_code_id: u64,
    pair_stable_code_id: Option<u64>,
) -> Addr {
    let mut msg = FactoryInstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        }],
        token_code_id,
        fee_address: None,
        generator_address: None,
        owner: String::from(OWNER),
        whitelist_code_id: 0,
    };

    if let Some(pair_stable_code_id) = pair_stable_code_id {
        msg.pair_configs.push(PairConfig {
            code_id: pair_stable_code_id,
            pair_type: PairType::Stable {},
            total_fee_bps: 100,
            maker_fee_bps: 10,
            is_disabled: false,
            is_generator_disabled: false,
        });
    }

    app.instantiate_contract(
        factory_code_id,
        Addr::unchecked(OWNER),
        &msg,
        &[],
        "Factory",
        None,
    )
    .unwrap()
}

fn instantiate_generator(
    app: &mut App,
    factory_instance: &Addr,
    astro_token_instance: &Addr,
    generator_controller: Option<String>,
) -> Addr {
    // Vesting
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let owner = Addr::unchecked(OWNER);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = VestingInstantiateMsg {
        owner: owner.to_string(),
        token_addr: astro_token_instance.to_string(),
    };

    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Vesting",
            None,
        )
        .unwrap();

    mint_tokens(
        app,
        owner.clone(),
        astro_token_instance,
        &owner,
        1_000_000_000_000_000,
    );

    // Generator
    let generator_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_generator::contract::execute,
            astroport_generator::contract::instantiate,
            astroport_generator::contract::query,
        )
        .with_reply_empty(astroport_generator::contract::reply),
    );

    let whitelist_code_id = store_whitelist_code(app);
    let generator_code_id = app.store_code(generator_contract);

    let init_msg = GeneratorInstantiateMsg {
        owner: owner.to_string(),
        factory: factory_instance.to_string(),
        guardian: None,
        start_block: Uint64::from(app.block_info().height),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::new(10_000000),
        vesting_contract: vesting_instance.to_string(),
        generator_controller,
        voting_escrow_delegation: None,
        voting_escrow: None,
        whitelist_code_id,
    };

    let generator_instance = app
        .instantiate_contract(
            generator_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Guage",
            None,
        )
        .unwrap();

    // Vesting to generator:
    let current_block = app.block_info();

    let amount = Uint128::new(63_072_000_000_000);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&VestingHookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: generator_instance.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: current_block.time.seconds(),
                        amount,
                    },
                    end_point: None,
                }],
            }],
        })
        .unwrap(),
        amount,
    };

    app.execute_contract(owner, astro_token_instance.clone(), &msg, &[])
        .unwrap();

    generator_instance
}

// fn instantiate_valkyrie_protocol(
//     app: &mut App,
//     valkyrie_token: &Addr,
//     pair: &Addr,
//     lp_token: &Addr,
// ) -> Addr {
//     // Valkyrie staking
//     let valkyrie_staking_contract = Box::new(ContractWrapper::new_with_empty(
//         valkyrie_lp_staking::entrypoints::execute,
//         valkyrie_lp_staking::entrypoints::instantiate,
//         valkyrie_lp_staking::entrypoints::query,
//     ));

//     let valkyrie_staking_code_id = app.store_code(valkyrie_staking_contract);

//     let init_msg = valkyrie::lp_staking::execute_msgs::InstantiateMsg {
//         token: valkyrie_token.to_string(),
//         pair: pair.to_string(),
//         lp_token: lp_token.to_string(),
//         whitelisted_contracts: vec![],
//         distribution_schedule: vec![
//             (
//                 app.block_info().height,
//                 app.block_info().height + 1,
//                 Uint128::new(50_000_000),
//             ),
//             (
//                 app.block_info().height + 1,
//                 app.block_info().height + 2,
//                 Uint128::new(60_000_000),
//             ),
//         ],
//     };

//     let valkyrie_staking_instance = app
//         .instantiate_contract(
//             valkyrie_staking_code_id,
//             Addr::unchecked(OWNER),
//             &init_msg,
//             &[],
//             "Valkyrie staking",
//             None,
//         )
//         .unwrap();

//     valkyrie_staking_instance
// }

// fn store_proxy_code(app: &mut App) -> u64 {
//     let generator_proxy_to_vkr_contract = Box::new(ContractWrapper::new_with_empty(
//         generator_proxy_to_vkr::contract::execute,
//         generator_proxy_to_vkr::contract::instantiate,
//         generator_proxy_to_vkr::contract::query,
//     ));

//     app.store_code(generator_proxy_to_vkr_contract)
// }

fn instantiate_proxy(
    app: &mut App,
    proxy_code: u64,
    generator_instance: &Addr,
    pair: &Addr,
    lp_token: &Addr,
    vkr_staking_instance: &Addr,
    vkr_token_instance: &Addr,
) -> Addr {
    let init_msg = ProxyInstantiateMsg {
        generator_contract_addr: generator_instance.to_string(),
        pair_addr: pair.to_string(),
        lp_token_addr: lp_token.to_string(),
        reward_contract_addr: vkr_staking_instance.to_string(),
        reward_token_addr: vkr_token_instance.to_string(),
    };

    app.instantiate_contract(
        proxy_code,
        Addr::unchecked(OWNER),
        &init_msg,
        &[],
        String::from("Proxy"),
        None,
    )
    .unwrap()
}

fn register_lp_tokens_in_generator(
    app: &mut App,
    generator_instance: &Addr,
    pools_with_proxy: Vec<PoolWithProxy>,
) {
    let pools: Vec<(String, Uint128)> = pools_with_proxy.iter().map(|p| p.pool.clone()).collect();

    app.execute_contract(
        Addr::unchecked(OWNER),
        generator_instance.clone(),
        &GeneratorExecuteMsg::SetupPools { pools },
        &[],
    )
    .unwrap();

    for pool_with_proxy in &pools_with_proxy {
        if let Some(proxy) = &pool_with_proxy.proxy {
            app.execute_contract(
                Addr::unchecked(OWNER),
                generator_instance.clone(),
                &GeneratorExecuteMsg::MoveToProxy {
                    lp_token: pool_with_proxy.pool.0.clone(),
                    proxy: proxy.to_string(),
                },
                &[],
            )
            .unwrap();
        }
    }
}

fn mint_tokens(app: &mut App, sender: Addr, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(sender, token.to_owned(), &msg, &[])
        .unwrap();
}

fn deposit_lp_tokens_to_generator(
    app: &mut App,
    generator_instance: &Addr,
    depositor: &str,
    lp_tokens: &[(&Addr, u128)],
) {
    for (token, amount) in lp_tokens {
        let msg = Cw20ExecuteMsg::Send {
            contract: generator_instance.to_string(),
            msg: to_binary(&GeneratorHookMsg::Deposit {}).unwrap(),
            amount: Uint128::from(amount.to_owned()),
        };

        app.execute_contract(Addr::unchecked(depositor), (*token).clone(), &msg, &[])
            .unwrap();
    }
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}

fn check_emission_balance(
    app: &mut App,
    generator: &Addr,
    lp_token: &Addr,
    user: &Addr,
    expected: u128,
) {
    let msg = GeneratorQueryMsg::UserVirtualAmount {
        lp_token: lp_token.to_string(),
        user: user.to_string(),
    };

    let res: Uint128 = app.wrap().query_wasm_smart(generator, &msg).unwrap();
    assert_eq!(Uint128::from(expected), res);
}

fn check_pending_rewards(
    app: &mut App,
    generator_instance: &Addr,
    token: &Addr,
    depositor: &str,
    (expected, expected_on_proxy): (u128, Option<Vec<u128>>),
) {
    let msg = GeneratorQueryMsg::PendingToken {
        lp_token: token.to_string(),
        user: String::from(depositor),
    };

    let res: PendingTokenResponse = app
        .wrap()
        .query_wasm_smart(generator_instance.to_owned(), &msg)
        .unwrap();

    assert_eq!(res.pending.u128(), expected);
    let pending_on_proxy = res.pending_on_proxy.map(|rewards| {
        rewards
            .into_iter()
            .map(|Asset { amount, .. }| amount.u128())
            .collect::<Vec<_>>()
    });
    assert_eq!(pending_on_proxy, expected_on_proxy)
}

fn create_pair(
    app: &mut App,
    factory: &Addr,
    pair_type: Option<PairType>,
    init_param: Option<Binary>,
    assets: Vec<AssetInfo>,
) -> (Addr, Addr) {
    app.execute_contract(
        Addr::unchecked(OWNER),
        factory.clone(),
        &FactoryExecuteMsg::CreatePair {
            pair_type: pair_type.unwrap_or(PairType::Xyk {}),
            asset_infos: assets.clone(),
            init_params: init_param,
        },
        &[],
    )
    .unwrap();

    let res: PairInfo = app
        .wrap()
        .query_wasm_smart(
            factory,
            &FactoryQueryMsg::Pair {
                asset_infos: assets,
            },
        )
        .unwrap();

    (res.contract_addr, res.liquidity_token)
}

fn store_whitelist_code(app: &mut App) -> u64 {
    let whitelist_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_whitelist::contract::execute,
        astroport_whitelist::contract::instantiate,
        astroport_whitelist::contract::query,
    ));

    app.store_code(whitelist_contract)
}
