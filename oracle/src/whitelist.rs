use crate::*;

use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::serde::{ Serialize, Deserialize };
use near_sdk::AccountId;
use near_sdk::collections::LookupMap;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct RequestorConfig {
    pub interface_name: String,
    pub account_id: AccountId, // Change to account_id
    pub stake_multiplier: Option<u16>, 
    pub code_base_url: Option<String>
}
   
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Whitelist(Option<LookupMap<AccountId, RequestorConfig>>); // maps requestor account id to requestors config

impl Whitelist {
    pub fn new(initial_whitelist: Option<Vec<RequestorConfig>>) -> Self {
        let mut whitelist: LookupMap<AccountId, RequestorConfig> = LookupMap::new(b"wlr".to_vec());

        match initial_whitelist {
            Some(initial_whitelist) => {
                // insert registry entry into whitelist
                for requestor in initial_whitelist {
                    whitelist.insert(&requestor.account_id, &requestor);
                    logger::log_whitelist(&requestor, true);
                }
                Self(Some(whitelist))
            }, 
            None => Self(None)
        }
    }

    pub fn contains(&self, requestor: AccountId) -> bool {
        match self.0.as_ref().expect("No whitelist initiated").get(&requestor) {
            None => false,
            _ => true
        }
    }
}

trait WhitelistHandler {
    fn add_to_whitelist(&mut self, new_requestor: RequestorConfig);
    fn remove_from_whitelist(&mut self, requestor: RequestorConfig);
    fn whitelist_contains(&self, requestor: AccountId) -> bool;
}

#[near_bindgen]
impl WhitelistHandler for Contract {
    
    #[payable]
    fn add_to_whitelist(&mut self, new_requestor: RequestorConfig) {
        self.assert_gov();

        let initial_storage = env::storage_usage();

        match &mut self.whitelist.0 {
            Some(whitelist) => {
                whitelist.insert(&new_requestor.account_id, &new_requestor);
            }, 
            None => {
                let mut whitelist: LookupMap<AccountId, RequestorConfig> = LookupMap::new(b"wlr".to_vec());
                whitelist.insert(&new_requestor.account_id, &new_requestor);
                self.whitelist = Whitelist(Some(whitelist));
            }
        };
      
        logger::log_whitelist(&new_requestor, true);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());
    }

    #[payable]
    fn remove_from_whitelist(&mut self, requestor: RequestorConfig) {
        self.assert_gov();

        let initial_storage = env::storage_usage();

        helpers::refund_storage(initial_storage, env::predecessor_account_id());
        logger::log_whitelist(&requestor, false);


        match &mut self.whitelist.0 {
            Some(whitelist) => {
                whitelist.remove(&requestor.account_id);
            }, 
            None => {
                panic!("Uninitiated whitelist")
            }
        };
    }

    fn whitelist_contains(&self, requestor: AccountId) -> bool {
        self.whitelist.contains(requestor)
    }
}

impl Contract {
    pub fn assert_whitelisted(&self, requestor: AccountId) {
        match self.whitelist.0 {
            Some(_) => {
                assert!(self.whitelist_contains(requestor), "Err predecessor is not whitelisted");
            }, 
            None => ()
        }
    }

    pub fn whitelist_get(&self, requestor: &AccountId) -> Option<RequestorConfig> {
        self.whitelist.0.as_ref().expect("No whitelist initiated").get(requestor)
    }
}


#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use near_sdk::{ MockedBlockchain };
    use near_sdk::{ testing_env, VMContext };
    use fee_config::FeeConfig;
    use super::*;

    fn alice() -> AccountId {
        "alice.near".to_string()
    }

    fn bob() -> AccountId {
        "bob.near".to_string()
    }

    fn carol() -> AccountId {
        "carol.near".to_string()
    }

    fn token() -> AccountId {
        "token.near".to_string()
    }

    fn gov() -> AccountId {
        "gov.near".to_string()
    }

    fn registry_entry(account: AccountId) -> RequestorConfig {
        RequestorConfig {
            interface_name: account.clone(),
            account_id: account.clone(),
            stake_multiplier: None,
            code_base_url: None
        }
    }

    fn config() -> oracle_config::OracleConfig {
        oracle_config::OracleConfig {
            gov: gov(),
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
            }
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
            attached_deposit: 1000 * 10u128.pow(24),
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 0,
        }
    }

    #[test]
    fn setting_initial_whitelist() {
        testing_env!(get_context(carol()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let contract = Contract::new(whitelist, config());
        let alice_is_whitelisted = contract.whitelist_contains(alice());
        let bob_is_whitelisted = contract.whitelist_contains(bob());
        let carol_is_whitelisted = contract.whitelist_contains(carol());
        assert!(!alice_is_whitelisted);
        assert!(bob_is_whitelisted);
        assert!(carol_is_whitelisted);
    }

    #[test]
    fn whitelist_add_remove() {
        testing_env!(get_context(gov()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        assert!(!contract.whitelist_contains(alice()));
        contract.add_to_whitelist(registry_entry(alice()));
        assert!(contract.whitelist_contains(alice()));
        contract.remove_from_whitelist(registry_entry(alice()));
        assert!(!contract.whitelist_contains(alice()));
    }

    #[test]
    fn whitelist_assert_passes_when_empty() {
        testing_env!(get_context(gov()));
        let whitelist = None;
        let contract = Contract::new(whitelist, config());
        contract.assert_whitelisted(alice());
    }

    #[test]
    #[should_panic(expected = "This method is only callable by the governance contract gov.near")]
    fn only_gov_can_add() {
        testing_env!(get_context(alice()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.add_to_whitelist(registry_entry(alice()));
    }

    #[test]
    #[should_panic(expected = "This method is only callable by the governance contract gov.near")]
    fn only_gov_can_remove() {
        testing_env!(get_context(alice()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.remove_from_whitelist(registry_entry(alice()));
    }
}
