use crate::*;
use flux_sdk::config::OracleConfig;
use near_sdk::AccountId;

#[near_bindgen]
impl Contract {
    pub fn get_config(&self) -> OracleConfig {
        self.configs.get(self.configs.len() - 1).unwrap()
    }

    #[payable]
    pub fn set_config(&mut self, new_config: OracleConfig) {
        // AUDIT: Validate account IDs and other data from the config.
        // SOLUTION: Find out what other configurations need to be set and add assertions
        self.assert_gov();

        let initial_storage = env::storage_usage();

        self.configs.push(&new_config);

        logger::log_oracle_config(&new_config, self.configs.len() - 1);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());
    }
}

impl Contract {
    pub fn assert_sender(&self, expected_sender: &AccountId) {
        assert_eq!(
            &env::predecessor_account_id(),
            expected_sender,
            "This function can only be called by {}",
            expected_sender
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use super::*;
    use flux_sdk::config::FeeConfig;
    use near_sdk::{json_types::U64, testing_env, MockedBlockchain, VMContext};

    fn alice() -> AccountId {
        "alice.near".to_string()
    }

    fn bob() -> AccountId {
        "bob.near".to_string()
    }

    fn token() -> AccountId {
        "token.near".to_string()
    }

    fn gov() -> AccountId {
        "gov.near".to_string()
    }

    fn config(gov: AccountId) -> oracle_config::OracleConfig {
        oracle_config::OracleConfig {
            gov,
            final_arbitrator: alice(),
            payment_token: token(),
            stake_token: token(),
            validity_bond: U128(0),
            max_outcomes: 8,
            default_challenge_window_duration: U64(1000),
            min_initial_challenge_window_duration: U64(1000),
            final_arbitrator_invoke_amount: U128(25_000_000_000_000_000_000_000_000_000_000),
            fee: FeeConfig {
                flux_market_cap: U128(50000),
                total_value_staked: U128(10000),
                resolution_fee_percentage: 5000, // 5%
            },
        }
    }

    fn get_context(predecessor_account_id: AccountId) -> VMContext {
        VMContext {
            current_account_id: token(),
            signer_account_id: bob(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 0,
            block_timestamp: 0,
            account_balance: 1000 * 10u128.pow(24),
            account_locked_balance: 0,
            storage_usage: 10u64.pow(6),
            attached_deposit: 19000000000000000000000,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 0,
        }
    }

    #[test]
    fn set_config_from_gov() {
        testing_env!(get_context(gov()));
        let mut contract = Contract::new(None, config(gov()));
        contract.set_config(config(alice()));
        assert_eq!(contract.get_config().gov, alice());
    }

    #[test]
    #[should_panic(expected = "This method is only callable by the governance contract gov.near")]
    fn fail_set_config_from_user() {
        testing_env!(get_context(alice()));
        let mut contract = Contract::new(None, config(gov()));
        contract.set_config(config(alice()));
    }
}
