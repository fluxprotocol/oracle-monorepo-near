#![allow(clippy::too_many_arguments)]

use near_sdk::{ AccountId, Balance, env, near_bindgen };
use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::collections::{ Vector, LookupMap };
use near_sdk::json_types::{ U64, U128 };

near_sdk::setup_alloc!();

pub mod types;
mod resolution_window;
pub mod data_request;
mod requestor_handler;
mod fungible_token_receiver;
pub mod callback_args;
pub mod whitelist;
pub mod oracle_config;
mod storage_manager;
mod helpers;
mod logger;
mod upgrade;
mod target_contract_handler;
pub mod fee_config;

/// Mocks
mod fungible_token;

pub use callback_args::*;

use types::*;
pub use data_request::{ DataRequest, Source };
use storage_manager::AccountStorageBalance;
use whitelist::RequestorConfig;

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize )]
pub struct Contract {
    pub whitelist: whitelist::Whitelist,
    pub configs: Vector<oracle_config::OracleConfig>,
    pub data_requests: Vector<DataRequest>,
    pub accounts: LookupMap<AccountId, AccountStorageBalance>, // storage map
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
        initial_whitelist: Option<Vec<RequestorConfig>>,
        config: oracle_config::OracleConfig,
    ) -> Self {
        let mut configs = Vector::new(b"c".to_vec());
        configs.push(&config);
        logger::log_oracle_config(&config, 0);

        Self {
            whitelist: whitelist::Whitelist::new(initial_whitelist),
            configs,
            data_requests: Vector::new(b"dr".to_vec()),
            accounts: LookupMap::new(b"a".to_vec()),
        }
    }
}

impl Contract {
    pub fn assert_gov(&self) {
        let config = self.configs.iter().last().unwrap();
        assert_eq!(
            config.gov,
            env::predecessor_account_id(),
            "This method is only callable by the governance contract {}",
            config.gov
        );
    }
}
