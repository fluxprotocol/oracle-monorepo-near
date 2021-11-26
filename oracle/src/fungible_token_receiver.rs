use crate::*;
use near_sdk::{
    serde::{ Serialize, Deserialize },
    serde_json,
    env,
    PromiseOrValue,
    json_types::ValidAccountId,
    near_bindgen,
};
use near_contract_standards::
    fungible_token::
    receiver::
    FungibleTokenReceiver;
use flux_sdk::{
    data_request::{NewDataRequestArgs, StakeDataRequestArgs},
    types::WrappedBalance,
};


#[derive(Serialize, Deserialize)]
pub enum Payload {
    NewDataRequest(NewDataRequestArgs),
    StakeDataRequest(StakeDataRequestArgs),
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    // @returns amount of unused tokens
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<WrappedBalance> {
        let sender: &AccountId = sender_id.as_ref();
        let initial_storage_usage = env::storage_usage();
        let account = self.get_storage_account(&sender);

        let payload: Payload = serde_json::from_str(&msg).expect("Failed to parse the payload, invalid `msg` format");
        let config = self.get_config();

        let unspent = match payload {
            Payload::NewDataRequest(payload) => {
                assert_eq!(config.payment_token, env::predecessor_account_id(), "ERR_WRONG_PAYMENT_TOKEN");
                self.ft_dr_new_callback(sender.clone(), amount.into(), payload).into()
            },
            Payload::StakeDataRequest(payload) => {
                assert_eq!(config.stake_token, env::predecessor_account_id(), "ERR_WRONG_STAKE_TOKEN");
                self.dr_stake(sender.clone(), amount.into(), payload)
            },
        };

        self.use_storage(&sender, initial_storage_usage, account.available);

        unspent
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use super::*;
    use crate::storage_manager::StorageManager;
    use flux_sdk::{
        config::{FeeConfig, OracleConfig},
        data_request::{DataRequestDataType, NewDataRequestArgs},
        outcome::{AnswerType, Outcome},
    };
    use near_sdk::{
        json_types::{ValidAccountId, U64},
        testing_env, MockedBlockchain, VMContext,
    };
    use std::convert::TryInto;

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

    fn to_valid(account: AccountId) -> ValidAccountId {
        account.try_into().expect("invalid account")
    }

    fn registry_entry(account: AccountId) -> Requester {
        Requester {
            contract_name: account.clone(),
            account_id: account.clone(),
            stake_multiplier: None,
            code_base_url: None,
        }
    }

    fn config() -> OracleConfig {
        OracleConfig {
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
            },
            min_resolution_bond: U128(100)
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
    #[should_panic(
        expected = "alice.near has 0 deposited, 3540000000000000000000 is required for this transaction"
    )]
    fn transfer_storage_no_funds() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            5,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: DataRequestDataType::String,
                provider: None
            },
        );

        let msg = serde_json::json!({
            "StakeDataRequest": {
                "id": "0",
                "outcome": Outcome::Answer(AnswerType::String("a".to_string()))
            }
        });
        contract.ft_on_transfer(alice().try_into().unwrap(), U128(100), msg.to_string());
    }

    #[test]
    fn transfer_storage_funds() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            5,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: DataRequestDataType::String,
                provider: None
            },
        );

        let storage_start = 10u128.pow(24);

        let mut c: VMContext = get_context(alice());
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
        contract.ft_on_transfer(alice().try_into().unwrap(), U128(100), msg.to_string());

        let account = contract.accounts.get(&alice());
        assert!(account.unwrap().available < storage_start);
    }
}
