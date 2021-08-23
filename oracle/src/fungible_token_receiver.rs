use crate::*;

use near_sdk::serde::{ Serialize, Deserialize };
use near_sdk::serde_json;
use near_sdk::PromiseOrValue;

#[derive(Serialize, Deserialize)]
pub enum Payload {
    NewDataRequest(NewDataRequestArgs),
    StakeDataRequest(StakeDataRequestArgs)
}

pub trait FungibleTokenReceiver {
    // @returns amount of unused tokens
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<WrappedBalance>;
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    // @returns amount of unused tokens
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String
    ) -> PromiseOrValue<WrappedBalance> {
        let initial_storage_usage = env::storage_usage();
        let account = self.get_storage_account(&sender_id);

        let payload: Payload =  serde_json::from_str(&msg).expect("Failed to parse the payload, invalid `msg` format");
        let unspent = match payload {
            Payload::NewDataRequest(payload) => self.ft_dr_new_callback(sender_id.clone(), amount.into(), payload).into(),
            Payload::StakeDataRequest(payload) => self.dr_stake(sender_id.clone(), amount.into(), payload),
        };

        self.use_storage(&sender_id, initial_storage_usage, account.available);

        unspent
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use super::*;
    use std::convert::TryInto;
    use near_sdk::json_types::ValidAccountId;
    use near_sdk::{ MockedBlockchain };
    use near_sdk::{ testing_env, VMContext };
    use crate::storage_manager::StorageManager;

    use fee_config::FeeConfig;

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

    fn target() -> AccountId {
        "target.near".to_string()
    }

    fn gov() -> AccountId {
        "gov.near".to_string()
    }

    fn to_valid(account: AccountId) -> ValidAccountId {
        account.try_into().expect("invalid account")
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
            final_arbitrator_invoke_amount: U128(250),
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
    #[should_panic(expected = "alice.near has 0 deposited, 4770000000000000000000 is required for this transaction")]
    fn transfer_storage_no_funds() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });

        let msg = serde_json::json!({
            "StakeDataRequest": {
                "id": "0",
                "outcome": Outcome::Answer(AnswerType::String("a".to_string()))
            }
        });
        contract.ft_on_transfer(alice(), U128(100), msg.to_string());
    }

    #[test]
    fn transfer_storage_funds() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });

        let storage_start = 10u128.pow(24);

        let mut c : VMContext = get_context(alice());
        c.attached_deposit = storage_start;
        testing_env!(c);
        contract.storage_deposit(Some(to_valid(alice())));

        testing_env!(get_context(token()));
        let msg = serde_json::json!({
            "StakeDataRequest": {
                "id": "0",
                "outcome": Outcome::Answer(AnswerType::String("a".to_string()))
            }
        });
        contract.ft_on_transfer(alice(), U128(100), msg.to_string());

        let account = contract.accounts.get(&alice());
        assert!(account.unwrap().available < storage_start);
    }
}
