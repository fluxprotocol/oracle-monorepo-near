use crate::*;
use crate::requester_handler::RequesterHandler;

use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::AccountId;
use near_sdk::collections::LookupMap;

use flux_sdk::requester::Requester;
   
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Whitelist(Option<LookupMap<AccountId, Requester>>); // maps requester account id to requesters config

impl Whitelist {
    pub fn new(initial_whitelist: Option<Vec<Requester>>) -> Self {
        let mut whitelist: LookupMap<AccountId, Requester> = LookupMap::new(b"wlr".to_vec());

        match initial_whitelist {
            Some(initial_whitelist) => {
                // insert registry entry into whitelist
                for requester in initial_whitelist {
                    whitelist.insert(&requester.account_id, &requester);
                    logger::log_whitelist(&requester, true);
                }
                Self(Some(whitelist))
            }, 
            None => Self(None)
        }
    }

    pub fn contains(&self, requester: AccountId) -> bool {
        match self.0.as_ref().expect("No whitelist initiated").get(&requester) {
            None => false,
            _ => true
        }
    }

    pub fn get_stake_multiplier(&self, requester: &AccountId) -> Option<u16> {
        match &self.0 {
            Some(whitelist) => {
                whitelist.get(requester).expect("not whitelisted").stake_multiplier
            },
            None => None
        }
    }

    pub fn whitelist_get_expect(&self, requester: &AccountId) -> Requester {
        match &self.0 {
            Some(whitelist) => {
                whitelist.get(requester).expect("requester not whitelisted")
            }, 
            None => RequesterHandler::new_no_whitelist(requester)
        }
    }
}

trait WhitelistHandler {
    fn add_to_whitelist(&mut self, new_requester: Requester);
    fn remove_from_whitelist(&mut self, requester: Requester);
    fn whitelist_contains(&self, requester: AccountId) -> bool;
}

#[near_bindgen]
impl WhitelistHandler for Contract {
    
    #[payable]
    fn add_to_whitelist(&mut self, new_requester: Requester) {
        self.assert_gov();


        match new_requester.stake_multiplier {
            Some(m) => assert!(m > 0, "stake multiplier can't be 0"),
            _ => ()
        };

        let initial_storage = env::storage_usage();

        match &mut self.whitelist.0 {
            Some(whitelist) => {
                whitelist.insert(&new_requester.account_id, &new_requester);
            }, 
            None => {
                let mut whitelist: LookupMap<AccountId, Requester> = LookupMap::new(b"wlr".to_vec());
                whitelist.insert(&new_requester.account_id, &new_requester);
                self.whitelist = Whitelist(Some(whitelist));
            }
        };
      
        logger::log_whitelist(&new_requester, true);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());
    }

    #[payable]
    fn remove_from_whitelist(&mut self, requester: Requester) {
        self.assert_gov();

        let initial_storage = env::storage_usage();

        helpers::refund_storage(initial_storage, env::predecessor_account_id());
        logger::log_whitelist(&requester, false);


        match &mut self.whitelist.0 {
            Some(whitelist) => {
                whitelist.remove(&requester.account_id);
            }, 
            None => {
                panic!("Uninitiated whitelist")
            }
        };
    }

    fn whitelist_contains(&self, requester: AccountId) -> bool {
        self.whitelist.contains(requester)
    }
}

impl Contract {


    pub fn assert_whitelisted(&self, requester: AccountId) {
        match self.whitelist.0 {
            Some(_) => {
                assert!(self.whitelist_contains(requester), "Err predecessor is not whitelisted");
            }, 
            None => ()
        }
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

    fn registry_entry(account: AccountId) -> Requester {
        Requester {
            contract_name: account.clone(),
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
