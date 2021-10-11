#![allow(clippy::too_many_arguments)]

use near_sdk::{
    AccountId,
    Balance, 
    env,
    near_bindgen,
    borsh::{ self, BorshDeserialize, BorshSerialize },
    collections::{ Vector, LookupMap },
    json_types::U128
};

near_sdk::setup_alloc!();

mod resolution_window;
pub mod data_request;
mod requester_handler;
mod fungible_token_receiver;
pub mod callback_args;
pub mod whitelist;
pub mod oracle_config;
mod storage_manager;
mod helpers;
mod logger;
mod upgrade;
pub mod fee_config;

/// Mocks
mod fungible_token;

pub use callback_args::*;

use storage_manager::AccountStorageBalance;
use flux_sdk::{
    data_request::DataRequest,
    config::OracleConfig,
    requester::Requester,
};

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize )]
pub struct Contract {
    pub whitelist: whitelist::Whitelist,
    pub configs: Vector<OracleConfig>,
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
        initial_whitelist: Option<Vec<Requester>>,
        config: OracleConfig,
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
        // AUDIT: .iter().last() might be slower than .get(len() - 1)
        // SOLUTION: implement .get(len() - 1)
        // let config = self.configs.get(self.configs.len() - 1).unwrap();
        let config = self.configs.iter().last().unwrap();
        assert_eq!(
            config.gov,
            env::predecessor_account_id(),
            "This method is only callable by the governance contract {}",
            config.gov
        );
    }
}
