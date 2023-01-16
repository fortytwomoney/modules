use abstract_boot::Abstract;
use abstract_os::{
    ans_host::ExecuteMsgFns,
    objects::{
        pool_id::PoolAddressBase, AssetEntry, LpToken, PoolMetadata, UncheckedContractEntry,
    },
};
use astroport::asset::AssetInfo;
use boot_core::{deploy::Deploy, BootError, Mock};
use cosmwasm_std::{Addr, Uint128};

use crate::{
    create_pair, instantiate_factory, instantiate_generator, instantiate_token, mint_tokens,
    register_lp_tokens_in_generator, store_factory_code, store_pair_code_id, store_token_code,
    ASTROPORT,
};

use super::OWNER;

#[derive(Debug, Clone)]
pub struct Astroport {
    pub generator: Addr,
    pub eur_usd_pair: Addr,
    pub eur_usd_lp: Addr,
    pub astro_token: Addr,
    pub eur_token: Addr,
    pub usd_token: Addr,
    pub token_code_id: u64,
}

impl Astroport {
    /// registers the Astroport contracts and assets on Abstract
    pub fn register_info_on_abstract(&self, abstrct: &Abstract<Mock>) -> Result<(), BootError> {
        let eur_asset = AssetEntry::new("eur");
        let usd_asset = AssetEntry::new("usd");
        let eur_usd_lp_asset = LpToken::new(ASTROPORT, vec!["eur", "usd"]);

        // Register addresses on ANS
        abstrct
            .ans_host
            .update_asset_addresses(
                vec![
                    (
                        eur_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.eur_token.to_string()),
                    ),
                    (
                        usd_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.usd_token.to_string()),
                    ),
                    (
                        eur_usd_lp_asset.to_string(),
                        cw_asset::AssetInfoBase::cw20(self.eur_usd_lp.to_string()),
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
                        format!("staking/{}", eur_usd_lp_asset),
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
                vec![(
                    PoolAddressBase::contract(self.eur_usd_pair.to_string()),
                    PoolMetadata::constant_product(
                        ASTROPORT,
                        vec![eur_asset.clone(), usd_asset.clone()],
                    ),
                )],
                vec![],
            )
            .unwrap();

        Ok(())
    }
}

// We can only deploy mock env for now
impl Deploy<Mock> for Astroport {
    type Error = BootError;

    fn deploy_on(chain: Mock, _version: impl Into<String>) -> Result<Self, Self::Error> {
        let mut app = chain.app.borrow_mut();

        let owner = Addr::unchecked(OWNER);

        let token_code_id = store_token_code(&mut app);
        let factory_code_id = store_factory_code(&mut app);
        let pair_code_id = store_pair_code_id(&mut app);

        let astro_token_instance = instantiate_token(
            &mut app,
            token_code_id,
            "ASTRO",
            Some(1_000_000_000_000_000),
        );
        let factory_instance =
            instantiate_factory(&mut app, factory_code_id, token_code_id, pair_code_id, None);

        let cny_eur_token_code_id = store_token_code(&mut app);
        let eur_token = instantiate_token(&mut app, cny_eur_token_code_id, "EUR", None);
        let usd_token = instantiate_token(&mut app, cny_eur_token_code_id, "USD", None);

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
            vec![PoolWithProxy {
                pool: (lp_eur_usd.to_string(), Uint128::from(50u32)),
                proxy: None,
            }],
        );

        // mint tokens to pair to have some liquidity
        mint_tokens(
            &mut app,
            owner.clone(),
            &eur_token,
            &pair_eur_usd,
            1_000_000,
        );

        mint_tokens(
            &mut app,
            owner.clone(),
            &usd_token,
            &pair_eur_usd,
            1_000_000,
        );

        Ok(Self {
            generator: generator_instance,
            eur_usd_pair: pair_eur_usd,
            eur_usd_lp: lp_eur_usd,
            astro_token: astro_token_instance,
            eur_token,
            usd_token,
            token_code_id,
        })
    }
}

pub struct PoolWithProxy {
    pub pool: (String, Uint128),
    pub proxy: Option<Addr>,
}
