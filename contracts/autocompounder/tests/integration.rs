use std::cell::RefCell;
use std::rc::Rc;

use abstract_boot::ManagerExecFns;
use abstract_os::ans_host::ExecuteMsgFns;
use abstract_os::objects::pool_id::PoolAddressBase;
use abstract_os::objects::{AssetEntry, LpToken, PoolMetadata, UncheckedContractEntry};
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

use boot_core::MockState;
use cosmwasm_std::{to_binary, Addr, Binary, Decimal, Empty, StdResult, Uint128, Uint64};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_multi_test::{App, ContractWrapper, Executor};
use forty_two::autocompounder::AUTOCOMPOUNDER;
use forty_two::cw_staking::CW_STAKING;

use test_utils::abstract_helper;

#[cfg(test)]
mod test_utils;

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";
const USER3: &str = "user3";
const USER4: &str = "user4";
const USER5: &str = "user5";
const USER6: &str = "user6";
const USER7: &str = "user7";
const USER8: &str = "user8";
const USER9: &str = "user9";
const ASTROPORT: &str = "astroport";

struct PoolWithProxy {
    pool: (String, Uint128),
    proxy: Option<Addr>,
}

#[test]
fn generator_without_reward_proxies() {
    use boot_core::prelude::*;

    let mut app = mock_app();

    let owner = Addr::unchecked(OWNER);
    let _user1 = Addr::unchecked(USER1);
    let _user2 = Addr::unchecked(USER2);

    let token_code_id = store_token_code(&mut app);
    let factory_code_id = store_factory_code(&mut app);
    let pair_code_id = store_pair_code_id(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));
    let factory_instance =
        instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

    let cny_eur_token_code_id = store_token_code(&mut app);
    let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
    let usd_token = instantiate_token(&mut app, cny_eur_token_code_id, "USD", None);
    let cny_token = instantiate_token(&mut app, cny_eur_token_code_id, "CNY", None);

    let (_pair_cny_eur, lp_cny_eur) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        vec![
            AssetInfo::Token {
                contract_addr: cny_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
        ],
    );

    let (pair_eur_usd, lp_eur_usd) = create_pair(
        &mut app,
        &factory_instance,
        None,
        None,
        vec![
            AssetInfo::Token {
                contract_addr: eur_token.clone(),
            },
            AssetInfo::Token {
                contract_addr: usd_token.clone(),
            },
        ],
    );

    let generator_instance =
        instantiate_generator(&mut app, &factory_instance, &astro_token_instance, None);

    register_lp_tokens_in_generator(
        &mut app,
        &generator_instance,
        vec![
            PoolWithProxy {
                pool: (lp_cny_eur.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
            PoolWithProxy {
                pool: (lp_eur_usd.to_string(), Uint128::from(50u32)),
                proxy: None,
            },
        ],
    );

    let mock_state = Rc::new(RefCell::new(MockState::new()));
    let app = Rc::new(RefCell::new(app));
    let mock = boot_core::Mock::new(&owner, &mock_state, &app).unwrap();
    let (mut deployment, mut os_core) = abstract_helper::init_abstract_env(&mock).unwrap();
    deployment.deploy(&mut os_core).unwrap();

    let eur_asset = AssetEntry::new("eur");
    let usd_asset = AssetEntry::new("usd");
    let eur_usd_lp_asset = LpToken::new(ASTROPORT, vec!["eur", "usd"]);

    // Register addresses on ANS
    deployment
        .ans_host
        .update_asset_addresses(
            vec![
                (
                    eur_asset.to_string(),
                    cw_asset::AssetInfoBase::cw20(eur_token.to_string()),
                ),
                (
                    usd_asset.to_string(),
                    cw_asset::AssetInfoBase::cw20(usd_token.to_string()),
                ),
                (
                    eur_usd_lp_asset.to_string(),
                    cw_asset::AssetInfoBase::cw20(lp_eur_usd.to_string()),
                ),
            ],
            vec![],
        )
        .unwrap();

    deployment
        .ans_host
        .update_contract_addresses(
            vec![(
                UncheckedContractEntry::new(
                    ASTROPORT.to_string(),
                    format!("staking/{}", eur_usd_lp_asset.to_string()),
                ),
                generator_instance.to_string(),
            )],
            vec![],
        )
        .unwrap();

    deployment
        .ans_host
        .update_dexes(vec![ASTROPORT.into()], vec![])
        .unwrap();
    deployment
        .ans_host
        .update_pools(
            vec![(
                PoolAddressBase::contract(pair_eur_usd.to_string()),
                PoolMetadata::constant_product(
                    ASTROPORT,
                    vec![eur_asset.clone(), usd_asset.clone()],
                ),
            )],
            vec![],
        )
        .unwrap();

    // set up the dex and staking contracts
    abstract_helper::init_exchange(&mock, &deployment, None).unwrap();
    abstract_helper::init_staking(&mock, &deployment, None).unwrap();

    let mut auto_compounder =
        forty_two_boot::autocompounder::AutocompounderApp::new(AUTOCOMPOUNDER, mock.clone());
    auto_compounder.as_instance_mut().set_mock(Box::new(
        ContractWrapper::new_with_empty(
            autocompounder::contract::execute,
            autocompounder::contract::instantiate,
            autocompounder::contract::query,
        )
        .with_reply_empty(::autocompounder::contract::reply),
    ));

    // upload and register autocompounder
    auto_compounder.upload().unwrap();
    deployment
        .version_control
        .register_apps(vec![auto_compounder.as_instance()], &deployment.version)
        .unwrap();

    let os = deployment
        .os_factory
        .create_default_os(
            abstract_os::objects::gov_type::GovernanceDetails::Monarchy {
                monarch: owner.to_string(),
            },
        )
        .unwrap();

    // install dex
    os.manager
        .install_module(EXCHANGE, Some(&Empty {}))
        .unwrap();

    // install staking
    os.manager
        .install_module(CW_STAKING, Some(&Empty {}))
        .unwrap();

    os.manager
        .install_module(
            AUTOCOMPOUNDER,
            Some(&abstract_os::app::InstantiateMsg {
                app: forty_two::autocompounder::AutocompounderInstantiateMsg {
                    code_id: token_code_id,
                    commission_addr: OWNER.to_string(),
                    deposit_fees: Decimal::zero(),
                    dex: ASTROPORT.to_string(),
                    fee_asset: eur_asset.to_string(),
                    performance_fees: Decimal::zero(),
                    pool_assets: vec![eur_asset, usd_asset],
                    withdrawal_fees: Decimal::zero(),
                },
                base: abstract_os::app::BaseInstantiateMsg {
                    ans_host_address: deployment.ans_host.addr_str().unwrap(),
                },
            }),
        )
        .unwrap();

    // create OS and install dex

    // // Mint tokens, so user can deposit
    // mint_tokens(&mut app, pair_cny_eur.clone(), &lp_cny_eur, &user1, 9);
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
            cap: cap.map(|v| Uint128::from(v)),
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
    mut app: &mut App,
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
        &mut app,
        owner.clone(),
        &astro_token_instance,
        &owner,
        1_000_000_000_000000,
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

    let whitelist_code_id = store_whitelist_code(&mut app);
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

    let amount = Uint128::new(63072000_000000);

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
            pair_type: pair_type.unwrap_or_else(|| PairType::Xyk {}),
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
