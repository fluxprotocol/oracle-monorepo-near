#![allow(clippy::too_many_arguments)]

use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::{LookupMap, Vector},
    env,
    json_types::U128,
    near_bindgen, AccountId, Balance,
};

near_sdk::setup_alloc!();

pub mod callback_args;
pub mod data_request;
pub mod fee_config;
mod fungible_token_receiver;
mod helpers;
mod logger;
pub mod oracle_config;
mod requester_handler;
mod resolution_window;
mod storage_manager;
mod upgrade;
pub mod whitelist;

/// Mocks
mod fungible_token;

pub use callback_args::*;

use flux_sdk::{config::OracleConfig, data_request::DataRequest, requester::Requester};
use storage_manager::AccountStorageBalance;

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Contract {
    pub whitelist: whitelist::Whitelist,
    pub configs: Vector<OracleConfig>,
    pub data_requests: Vector<DataRequest>,
    pub accounts: LookupMap<AccountId, AccountStorageBalance>, // storage map
    pub paused: bool
}

impl Default for Contract {
    fn default() -> Self {
        env::panic(b"Contract should be initialized before usage")
    }
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(initial_whitelist: Option<Vec<Requester>>, config: OracleConfig) -> Self {
        let mut configs = Vector::new(b"c".to_vec());
        configs.push(&config);
        logger::log_oracle_config(&config, 0);

        Self {
            whitelist: whitelist::Whitelist::new(initial_whitelist),
            configs,
            data_requests: Vector::new(b"dr".to_vec()),
            accounts: LookupMap::new(b"a".to_vec()),
            paused: false
        }
    }
}

impl Contract {
    pub fn assert_gov(&self) {
        let config = self.configs.get(self.configs.len() - 1).unwrap();
        assert_eq!(
            config.gov,
            env::predecessor_account_id(),
            "This method is only callable by the governance contract {}",
            config.gov
        );
    }
    pub fn assert_unpaused(&self) {
        assert!(!self.paused, "Oracle is paused");
    }
    pub fn assert_sender(&self, expected_sender: &AccountId) {
        assert_eq!(
            &env::predecessor_account_id(),
            expected_sender,
            "This function can only be called by {}",
            expected_sender
        );
    }
}
