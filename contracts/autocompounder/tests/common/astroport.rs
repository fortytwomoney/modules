use boot_core::{
    prelude::{BootInstantiate, BootUpload},
    BootEnvironment,
};

pub fn init_astroport<Chain: BootEnvironment>(
    chain: &Chain,
) -> (pair::Pair<Chain>, generator::Generator<Chain>) {
    let pair = pair::Pair::new("pair", chain.clone());
    let generator = generator::Generator::new("generator", chain.clone());

    pair.upload()?;
    generator.upload()?;

    pair.instantiate(pair::InstantiateMsg {})(pair, generator)
}

pub mod pair {
    use astroport::asset::AssetInfo;
    pub use astroport::pair::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
    use boot_core::{BootEnvironment, Contract};
    use cw_multi_test::ContractWrapper;

    #[boot_core::boot_contract(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
    pub struct Pair;
    // implement chain-generic functions
    impl<Chain: BootEnvironment> Pair<Chain> {
        pub fn new(id: &str, chain: Chain) -> Self {
            Self(
                Contract::new(id, chain).with_mock(Box::new(ContractWrapper::new_with_empty(
                    cw20_base::contract::execute,
                    cw20_base::contract::instantiate,
                    cw20_base::contract::query,
                ))), // .with_wasm_path(file_path),
            )
        }

        pub fn init_msg(&self, token_code_id: u64) -> InstantiateMsg {
            InstantiateMsg {
                asset_infos: vec![
                    AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                ],
                token_code_id,
                factory_addr: "factory".to_string(),
                init_params: None,
            }
        }
    }
}

pub mod generator {
    pub use astroport::generator::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
    use boot_core::{BootEnvironment, Contract};
    use cosmwasm_std::{Addr, Uint128, Uint64};
    use cw_multi_test::ContractWrapper;

    #[boot_core::boot_contract(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
    pub struct Generator;
    // implement chain-generic functions
    impl<Chain: BootEnvironment> Generator<Chain> {
        pub fn new(id: &str, chain: Chain) -> Self {
            Self(
                Contract::new(id, chain).with_mock(Box::new(ContractWrapper::new_with_empty(
                    cw20_base::contract::execute,
                    cw20_base::contract::instantiate,
                    cw20_base::contract::query,
                ))), // .with_wasm_path(file_path),
            )
        }

        pub fn init_msg(&self, astro_token: Addr) -> InstantiateMsg {
            InstantiateMsg {
                owner: todo!(),
                factory: todo!(),
                generator_controller: None,
                voting_escrow_delegation: None,
                voting_escrow: None,
                guardian: None,
                astro_token: astro_token.to_string(),
                tokens_per_block: Uint128::from(100u128),
                start_block: Uint64::from(0u64),
                vesting_contract: todo!(),
                whitelist_code_id: todo!(),
            }
        }
    }
}

pub mod factory {
    use astroport::factory::PairConfig;
    pub use astroport::factory::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
    use boot_core::{BootEnvironment, Contract, prelude::ContractInstance, BootError};
    use cosmwasm_std::Addr;
    use cw_multi_test::ContractWrapper;

    #[boot_core::boot_contract(InstantiateMsg, ExecuteMsg, QueryMsg, MigrateMsg)]
    pub struct Generator;
    // implement chain-generic functions
    impl<Chain: BootEnvironment> Generator<Chain> {
        pub fn new(id: &str, chain: Chain) -> Self {
            Self(
                Contract::new(id, chain).with_mock(Box::new(ContractWrapper::new_with_empty(
                    cw20_base::contract::execute,
                    cw20_base::contract::instantiate,
                    cw20_base::contract::query,
                ))), // .with_wasm_path(file_path),
            )
        }

        pub fn init_msg(&self, token_code_id: u64, generator_addr: Addr) -> Result<InstantiateMsg,BootError> {
            Ok(InstantiateMsg {
                            pair_configs: vec![PairConfig {
                                code_id: self.code_id()?,
                                pair_type: astroport::factory::PairType::Stable {  },
                                total_fee_bps: 0,
                                maker_fee_bps: 0,
                                is_disabled: false,
                                is_generator_disabled: false,
                            }],
                            token_code_id,
                            fee_address: None,
                            generator_address: Some(generator_addr.to_string()),
                            owner: self.0.get_chain().sender().to_string(),
                            whitelist_code_id: 0,
                        })
        }
    }
}
