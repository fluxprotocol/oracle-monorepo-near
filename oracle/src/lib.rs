#![allow(clippy::too_many_arguments)]

use near_sdk::{ AccountId, Balance, env, near_bindgen };
use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::collections::{ Vector, LookupMap };
use near_sdk::json_types::{ ValidAccountId, U64, U128 };

near_sdk::setup_alloc!();

mod types;
mod data_request;
mod fungible_token_receiver;
pub mod callback_args;
mod whitelist;
pub mod oracle_config;
mod storage_manager;
mod helpers;
mod logger;
mod upgrade;
mod target_contract;

/// Mocks
mod mock_requestor;
mod fungible_token;

use callback_args::*;

use types::*;
use data_request::{ DataRequest };
use storage_manager::AccountStorageBalance;

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize )]
pub struct Contract {
    pub whitelist: whitelist::Whitelist,
    pub configs: Vector<oracle_config::OracleConfig>,
    pub data_requests: Vector<DataRequest>,
    pub validity_bond: U128,
    // Storage map
    pub accounts: LookupMap<AccountId, AccountStorageBalance>
}

impl Default for Contract {
    fn default() -> Self {
        env::panic(b"Contract should be initialized before usage")
    }
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        initial_whitelist: Option<Vec<ValidAccountId>>,
        config: oracle_config::OracleConfig
    ) -> Self {
        let mut configs = Vector::new(b"c".to_vec());
        configs.push(&config);
        logger::log_oracle_config(&config, 0);

        Self {
            whitelist: whitelist::Whitelist::new(initial_whitelist),
            configs,
            data_requests: Vector::new(b"dr".to_vec()),
            validity_bond: 1.into(),
            accounts: LookupMap::new(b"a".to_vec()),
        }
    }
}

impl Contract {
    fn assert_gov(&self) {
        let config = self.configs.iter().last().unwrap();
        assert_eq!(
            config.gov,
            env::predecessor_account_id(),
            "This method is only callable by the governance contract {}",
            config.gov
        );
    }
}