use near_sdk::{env, near_bindgen, AccountId, ext_contract};
use near_sdk::json_types::{U64, U128};
use near_sdk::collections::LookupMap;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{ Deserialize, Serialize };

near_sdk::setup_alloc!();
pub const PERCENTAGE_DIVISOR: u16 = 10_000;

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct AnswerNumberType {
    pub value: U128,
    pub multiplier: U128,
    pub negative: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum AnswerType {
    Number(AnswerNumberType),
    String(String)
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum Outcome {
    Answer(AnswerType),
    Invalid
}

#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Clone)]
pub enum DataRequestStatus {
    Pending,
    Finalized(Outcome)
}

#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Clone)]
pub struct DataRequest {
    status: DataRequestStatus,
    tags: Option<Vec<String>>
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct TargetContract {
    pub oracle: AccountId,
    pub fee_token: AccountId,
    pub requestor: AccountId,
    pub data_requests: LookupMap<U64, DataRequest>
}

impl Default for TargetContract {
    fn default() -> Self {
        env::panic(b"Contract should be initialized before usage")
    }
}

// Private methods
impl TargetContract {
    pub fn assert_oracle(&self) {
        assert_eq!(&env::predecessor_account_id(), &self.oracle, "ERR_INVALID_ORACLE_ADDRESS");
    }
    pub fn assert_requestor(&self, requestor: AccountId) {
        assert_eq!(requestor, self.requestor, "ERR_WRONG_REQUESTOR");
    }
}

#[ext_contract]
trait ExtTokenContract {
    fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> Promise;
}

#[near_bindgen]
impl TargetContract {
    #[init]
    pub fn new(
        oracle: AccountId,
        fee_token: AccountId,
        requestor: AccountId
    ) -> Self {
        Self {
            oracle,
            fee_token,
            requestor,
            data_requests: LookupMap::new(b"d".to_vec())
        }
    }
    
    /**
     * @notice called by oracle to finalize the outcome result of a data request
     */
    #[payable]
    pub fn set_outcome(
        &mut self,
        request_id: U64,
        requestor: AccountId,
        outcome: Outcome,
        tags: Option<Vec<String>>,
    ) {
        self.assert_oracle();
        self.assert_requestor(requestor.clone());
        assert_eq!(env::attached_deposit(), 1);

        // insert finalized data request outcome into this contract
        let result = DataRequest {
            status: DataRequestStatus::Finalized(outcome),
            tags
        };
        self.data_requests.insert(
            &request_id,
            &result
        );
    }

    pub fn get_outcome(
        self,
        request_id: U64
    ) -> Option<Outcome> {
        let dr = self.data_requests.get(&request_id);

        if dr.is_none() {
            return None;
        }
        
        match dr.unwrap().status {
            DataRequestStatus::Pending => None,
            DataRequestStatus::Finalized(s) => Some(s),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    fn alice() -> AccountId {
        "alice.near".to_string()
    }

    fn oracle() -> AccountId {
        "oracle.near".to_string()
    }
    
    fn fee_token() -> AccountId {
        "fee_token.near".to_string()
    }

    fn requestor() -> AccountId {
        "requestor.near".to_string()
    }

    fn get_context(predecessor_account_id: AccountId) -> VMContext {
        VMContext {
            current_account_id: alice(),
            signer_account_id: alice(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 1,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 0,
        }
    }

    fn set_outcome(contract: &mut TargetContract) {
        contract.set_outcome(
            U64(0),
            requestor(),
            Outcome::Answer(AnswerType::String("outcome".to_string())),
            None
        );
    }
    
    #[test]
    fn tc_outcome_initialized() {
        let context = get_context(alice());
        testing_env!(context);
        let contract = TargetContract::new(
            oracle(),
            fee_token(),
            requestor()
        );
        assert_eq!(contract.data_requests.get(&U64(0)), None);
    }

    #[test]
    #[should_panic(expected = "ERR_INVALID_ORACLE_ADDRESS")]
    fn tc_set_outcome_not_oracle() {
        let context = get_context(alice());
        testing_env!(context);
        let mut contract = TargetContract::new(
            oracle(),
            fee_token(),
            requestor()
        );
        set_outcome(&mut contract);
    }
    
    #[test]
    fn tc_set_outcome_success() {
        let context = get_context(oracle());
        testing_env!(context);
        let mut contract = TargetContract::new(
            oracle(),
            fee_token(),
            requestor()
        );
        assert_eq!(contract.data_requests.get(&U64(0)), None);
        set_outcome(&mut contract);
        let expected_dr = DataRequest {
            status: DataRequestStatus::Finalized(Outcome::Answer(AnswerType::String("outcome".to_string()))),
            tags: None
        };
        assert_eq!(contract.data_requests.get(&U64(0)), Some(expected_dr));
    }
}
