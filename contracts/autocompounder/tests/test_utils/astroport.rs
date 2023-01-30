use super::OWNER;
use crate::{
    create_pair, increase_allowance, instantiate_factory, instantiate_generator, instantiate_token,
    mint_tokens, register_lp_tokens_in_generator, store_factory_code, store_pair_code_id,
    store_token_code, ASTROPORT,
};
use abstract_boot::Abstract;
use abstract_os::{
    ans_host::ExecuteMsgFns,
    objects::{
        pool_id::PoolAddressBase, AssetEntry, LpToken, PoolMetadata,
        UncheckedContractEntry,
    },
};
use astroport::asset::{Asset, AssetInfo};
use boot_core::{
    deploy::Deploy, prelude::ContractInstance, state::StateInterface, BootError, Mock,
};
use boot_cw_plus::Cw20;
use cosmwasm_std::{Addr, Empty, Uint128};
use cw_multi_test::{App, Executor};

pub const GENERATOR: &str = "astroport:generator";
pub const FACTORY: &str = "astroport:factory";
pub const ASTRO_TOKEN: &str = "astro";
pub const EUR_USD_PAIR: &str = "astroport:eur_usd_pair";
pub const EUR_USD_LP: &str = "astroport?eur,usd";
pub const ASTRO_EUR_PAIR: &str = "astroport:astro_eur_pair";
pub const ASTRO_EUR_LP: &str = "astroport?astro,eur";
pub const EUR_TOKEN: &str = "eur";
pub const USD_TOKEN: &str = "usd";

#[derive(Clone)]
pub struct Astroport {
    pub generator: Addr,
    pub eur_usd_pair: Addr,
    pub eur_usd_lp: Cw20<Mock>,
    pub astro_eur_pair: Addr,
    pub astro_eur_lp: Cw20<Mock>,
    pub astro_token: Cw20<Mock>,
    pub eur_token: Cw20<Mock>,
    pub usd_token: Cw20<Mock>,
}

impl Astroport {
    /// registers the Astroport contracts and assets on Abstract
    pub(crate) fn register_info_on_abstract(
        &self,
        abstrct: &Abstract<Mock>,
    ) -> Result<(), BootError> {
        let eur_asset = AssetEntry::new(EUR_TOKEN);
        let usd_asset = AssetEntry::new(USD_TOKEN);
        let astro_asset = AssetEntry::new(ASTRO_TOKEN);
        let eur_usd_lp_asset = LpToken::new(ASTROPORT, vec![EUR_TOKEN, USD_TOKEN]);
        let eur_astro_lp_asset = LpToken::new(ASTROPORT, vec![ASTRO_TOKEN, EUR_TOKEN]);

        // Register addresses on ANS
        abstrct
            .ans_host
            .update_asset_addresses(
                vec![
                    (
                        eur_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.eur_token.address()?),
                    ),
                    (
                        usd_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.usd_token.address()?),
                    ),
                    (
                        eur_usd_lp_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.eur_usd_lp.address()?),
                    ),
                    (
                        eur_astro_lp_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.astro_eur_lp.address()?),
                    ),
                    (
                        ASTRO_TOKEN.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.astro_token.address()?),
                    ),
                ],
                vec![],
            )
            .unwrap();

        abstrct
            .ans_host
            .update_contract_addresses(
                vec![(
                    UncheckedContractEntry::new(
                        ASTROPORT.to_string(),
                        format!("staking/{eur_usd_lp_asset}"),
                    ),
                    self.generator.to_string(),
                )],
                vec![],
            )
            .unwrap();

        abstrct
            .ans_host
            .update_dexes(vec![ASTROPORT.into()], vec![])
            .unwrap();
        abstrct
            .ans_host
            .update_pools(
                vec![
                    (
                        PoolAddressBase::contract(self.eur_usd_pair.to_string()),
                        PoolMetadata::constant_product(
                            ASTROPORT,
                            vec![eur_asset.clone(), usd_asset],
                        ),
                    ),
                    (
                        PoolAddressBase::contract(self.astro_eur_pair.to_string()),
                        PoolMetadata::constant_product(
                            ASTROPORT,
                            vec![astro_asset, eur_asset],
                        ),
                    ),
                ],
                vec![],
            )
            .unwrap();

        Ok(())
    }
}

// We can only deploy mock env for now
impl Deploy<Mock> for Astroport {
    type Error = BootError;
    type DeployData = Empty;

    fn deploy_on(chain: Mock, _: Empty) -> Result<Self, Self::Error> {
        let eur_usd_lp: Cw20<Mock> = Cw20::new(EUR_USD_LP, chain.clone());
        let astro_eur_lp: Cw20<Mock> = Cw20::new(ASTRO_EUR_LP, chain.clone());
        let astro_token: Cw20<Mock> = Cw20::new(ASTRO_TOKEN, chain.clone());
        let eur_token: Cw20<Mock> = Cw20::new(EUR_TOKEN, chain.clone());
        let usd_token: Cw20<Mock> = Cw20::new(USD_TOKEN, chain.clone());

        let mut app = chain.app.borrow_mut();
        let state = chain.state.clone();

        let owner = Addr::unchecked(OWNER);

        let token_code_id = store_token_code(&mut app);
        astro_token.set_code_id(token_code_id);

        let factory_code_id = store_factory_code(&mut app);
        let pair_code_id = store_pair_code_id(&mut app);

        let astro_token_instance = instantiate_token(
            &mut app,
            token_code_id,
            "ASTRO",
            None,
        );
        astro_token.set_address(&astro_token_instance);

        let factory_instance =
            instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);
        state.borrow_mut().set_address(FACTORY, &factory_instance);

        let usd_eur_token_code_id = store_token_code(&mut app);
        let eur_token_addr = instantiate_token(&mut app, usd_eur_token_code_id, "EUR", None);
        eur_token.set_address(&eur_token_addr);
        let usd_token_addr = instantiate_token(&mut app, usd_eur_token_code_id, "USD", None);
        usd_token.set_address(&usd_token_addr);

        let (pair_eur_usd, lp_eur_usd) = create_pair(
            &mut app,
            &factory_instance,
            None,
            None,
            vec![
                AssetInfo::Token {
                    contract_addr: eur_token_addr.clone(),
                },
                AssetInfo::Token {
                    contract_addr: usd_token_addr.clone(),
                },
            ],
        );
        state.borrow_mut().set_address(EUR_USD_PAIR, &pair_eur_usd);
        eur_usd_lp.set_address(&lp_eur_usd);

        let (pair_astro_eur, lp_astro_eur) = create_pair(
            &mut app,
            &factory_instance,
            None,
            None,
            vec![
                AssetInfo::Token {
                    contract_addr: astro_token_instance.clone(),
                },
                AssetInfo::Token {
                    contract_addr: eur_token_addr.clone(),
                },
            ],
        );
        // save pair address and lp token address
        state
            .borrow_mut()
            .set_address(ASTRO_EUR_PAIR, &pair_astro_eur);
        astro_eur_lp.set_address(&lp_astro_eur);

        let generator_instance =
            instantiate_generator(&mut app, &factory_instance, &astro_token_instance, None);
        state
            .borrow_mut()
            .set_address(GENERATOR, &generator_instance);
        register_lp_tokens_in_generator(
            &mut app,
            &generator_instance,
            vec![PoolWithProxy {
                pool: (lp_eur_usd.to_string(), Uint128::from(50u32)),
                proxy: None,
            }],
        );

        // give user some funds
        let astro_user = Addr::unchecked("astro_user");
        provide_initial_liquidlity(&mut app, &owner, &eur_token_addr, 1_000_000, &usd_token_addr,1_000_000, &pair_eur_usd, &astro_user);
        provide_initial_liquidlity(&mut app, &owner, &astro_token_instance, 100_000_000, &eur_token_addr, 10_000_000, &pair_astro_eur, &astro_user);

        // drop the mutable borrow of app
        // This allows us to pass `chain` to load Abstract
        drop(app);

        let astroport = Self {
            generator: generator_instance,
            eur_usd_pair: pair_eur_usd,
            astro_eur_pair: pair_astro_eur,
            astro_eur_lp,
            eur_usd_lp,
            astro_token,
            eur_token,
            usd_token,
        };

        // register contracts in abstract host
        let abstract_ = Abstract::load_from(chain)?;
        astroport.register_info_on_abstract(&abstract_)?;

        Ok(astroport)
    }

    // Loads Astroport addresses from state
    fn load_from(chain: Mock) -> Result<Self, Self::Error> {
        let state = chain.state.borrow();
        // load all addresses for Self from state
        let generator_instance = state.get_address(GENERATOR)?;
        let eur_usd_pair = state.get_address(EUR_USD_PAIR)?;
        let eur_usd_lp: Cw20<Mock> = Cw20::new(EUR_USD_LP, chain.clone());
        let astro_eur_pair = state.get_address(ASTRO_EUR_PAIR)?;
        let astro_eur_lp: Cw20<Mock> = Cw20::new(ASTRO_EUR_LP, chain.clone());
        let astro_token: Cw20<Mock> = Cw20::new(ASTRO_TOKEN, chain.clone());
        let eur_token: Cw20<Mock> = Cw20::new(EUR_TOKEN, chain.clone());
        let usd_token: Cw20<Mock> = Cw20::new(USD_TOKEN, chain.clone());

        Ok(Self {
            generator: generator_instance,
            eur_usd_pair,
            eur_usd_lp,
            astro_eur_pair,
            astro_eur_lp,
            astro_token,
            eur_token,
            usd_token,
        })
    }
}

fn provide_initial_liquidlity(
    app: &mut std::cell::RefMut<App>,
    owner: &Addr,
    asset1: &Addr,
    amount1: u128,
    asset2: &Addr,
    amount2: u128,
    pair_addr: &Addr,
    receiver: &Addr,
) {
    mint_tokens(app, owner.clone(), asset1, owner, amount1);

    mint_tokens(app, owner.clone(), asset2, owner, amount2);

    increase_allowance(app, owner.clone(), asset1, pair_addr, amount1);

    increase_allowance(app, owner.clone(), asset2, pair_addr, amount2);

    // add liquidity and mint the liquidity tokens to the astro user, so that the owner has no funds.
    provide_liquidity(
        app,
        owner,
        receiver,
        pair_addr,
        vec![
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset1.clone(),
                },
                amount: Uint128::from(amount1),
            },
            Asset {
                info: AssetInfo::Token {
                    contract_addr: asset2.clone(),
                },
                amount: Uint128::from(amount2),
            },
        ],
    );
}

pub struct PoolWithProxy {
    pub pool: (String, Uint128),
    pub proxy: Option<Addr>,
}

fn provide_liquidity(
    app: &mut App,
    sender: &Addr,
    receiver: &Addr,
    pair: &Addr,
    assets: Vec<Asset>,
) {
    let msg = astroport::pair::ExecuteMsg::ProvideLiquidity {
        assets,
        slippage_tolerance: None,
        auto_stake: Some(false),
        receiver: Some(receiver.to_string()),
    };

    app.execute_contract(sender.clone(), pair.clone(), &msg, &[])
        .unwrap();
}
