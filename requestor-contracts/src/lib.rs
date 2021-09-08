use near_sdk::{env, log, near_bindgen, AccountId, Balance, Promise, ext_contract};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{ Deserialize, Serialize };
use near_sdk::serde_json::json;
use near_sdk::json_types::{U64, U128, ValidAccountId};
use near_sdk::collections::{UnorderedSet, LookupMap};
use fungible_token_handler::fungible_token_transfer_call;

mod fungible_token_handler;

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone)]
pub struct Source {
    pub end_point: String,
    pub source_path: String
}


#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum AnswerType {
    Number(AnswerNumberType),
    String
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum DataRequestDataType {
    Number(U128),
    String,
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct AnswerNumberType {
    pub value: U128,
    pub multiplier: U128,
    pub negative: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum Outcome {
    Answer(AnswerType),
    Invalid
}

#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Clone, Deserialize, Serialize)]
pub enum DataResponseStatus {
    Pending,
    Finalized(Outcome)
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
pub struct DataRequest {
    amount: WrappedBalance,
    payload: NewDataRequestArgs
}

#[derive(BorshSerialize, BorshDeserialize, Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct DataResponse {
    status: DataResponseStatus,
    tags: Vec<String>
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
pub struct NewDataRequestArgs {
    pub sources: Option<Vec<Source>>,
    pub tags: Option<Vec<String>>,
    pub description: Option<String>,
    pub outcomes: Option<Vec<String>>,
    pub challenge_period: WrappedTimestamp,
    pub data_type: DataRequestDataType,
    pub creator: AccountId,
}

#[ext_contract]
trait OracleContract {
    fn get_outcome(&self, dr_id: U64);
}

near_sdk::setup_alloc!();

pub type WrappedTimestamp = U64;
pub type WrappedBalance = U128;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct RequestorContract {
    pub oracle: AccountId,
    pub stake_token: AccountId,
    pub nonce: u64,
    pub data_requests: LookupMap<u64, DataRequest>,
    pub data_responses: LookupMap<u64, DataResponse>,
    pub whitelist: UnorderedSet<AccountId> // accounts allowed to call create_data_request(). if len() == 0, no whitelist (any account can make data request)
}

impl Default for RequestorContract {
    fn default() -> Self {
        env::panic(b"Contract should be initialized before usage")
    }
}

// Private methods
impl RequestorContract {
    pub fn assert_oracle(&self) {
        assert_eq!(&env::predecessor_account_id(), &self.oracle, "ERR_INVALID_ORACLE_ADDRESS");
    }
    // if whitelist is populated, make sure caller's account is included in it
    pub fn assert_whitelisted(&self) {
        if self.whitelist.len() > 0 {
            assert!(
                self.whitelist.contains(&env::predecessor_account_id()),
                "ERR_NOT_WHITELISTED"
            )
        }
    }

    fn get_nonce(&mut self) -> u64 {
        self.nonce += 1;
        self.nonce
    }   
}

#[near_bindgen]
impl RequestorContract {
    #[init]
    pub fn new(
        oracle: AccountId,
        stake_token: AccountId,
        whitelist: Option<Vec<ValidAccountId>>,
    ) -> Self {
        let mut requestor_instance = Self {
            oracle,
            stake_token,
            nonce: 0,
            data_requests: LookupMap::new(b"drq".to_vec()),
            data_responses: LookupMap::new(b"drs".to_vec()),
            whitelist: UnorderedSet::new(b"w".to_vec())
        };

        // populate whitelist
        if let Some(whitelist) = whitelist {
            for acct in whitelist {
                requestor_instance.whitelist.insert(acct.as_ref());
            }
        }

        requestor_instance
    }

    #[payable]
    pub fn create_data_request(
        &mut self,
        amount: WrappedBalance,
        mut payload: NewDataRequestArgs
    ) -> Promise {
        self.assert_whitelisted();
        let nonce = self.get_nonce();

        // insert nonce into tags
        let mut tags = payload.tags.unwrap_or(vec![]);
        tags.push(nonce.to_string());
        payload.tags = Some(tags);

        let dr = DataRequest{
            amount,
            payload: payload.clone()
        };
        self.data_requests.insert(&nonce, &dr);
        log!("storing data request under {}", nonce);
        fungible_token_transfer_call(
            self.stake_token.clone(),
            self.oracle.clone(),
            amount.into(),
            json!({"NewDataRequest": payload}).to_string() 
        )
    }

    /**
     * @notice called by oracle to finalize the outcome result of a data request
     */
    #[payable]
    pub fn set_outcome(
        &mut self,
        requestor: AccountId,
        outcome: Outcome,
        tags: Vec<String>,
    ) {
        self.assert_oracle();
        assert_eq!(env::current_account_id(), requestor, "can only set outcomes for requests that are initiated by this requestor");
        assert_eq!(env::attached_deposit(), 1);

        // insert finalized data request outcome into this contract
        let result = DataResponse {
            status: DataResponseStatus::Finalized(outcome),
            tags: tags.clone()
        };
        self.data_responses.insert(
            &tags.last().unwrap().parse::<u64>().unwrap(),
            &result
        );
    }
    
    #[payable]
    pub fn get_outcome(
        &mut self,
        request_id: U64,
    ) -> Promise {
        oracle_contract::get_outcome(
            request_id, 
            &self.oracle.as_str(),
            0,
            1_000_000_000_000
        )
    }

    pub fn get_data_request(&self, nonce: U64) -> Option<DataRequest> {
        self.data_requests.get(&u64::from(nonce))
    }

    pub fn get_data_response(&self, nonce: U64) -> Option<DataResponse> {
        self.data_responses.get(&u64::from(nonce))
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::serde_json;
    use near_sdk::{testing_env, VMContext};

    fn alice() -> AccountId {
        "alice.near".to_string()
    }

    fn oracle() -> AccountId {
        "oracle.near".to_string()
    }

    fn token() -> AccountId {
        "token.near".to_string()
    }

    fn get_context(input: Vec<u8>, is_view: bool) -> VMContext {
        VMContext {
            current_account_id: alice(),
            signer_account_id: alice(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: alice(),
            input,
            block_index: 0,
            block_timestamp: 0,
            account_balance: 10000 * 10u128.pow(24),
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
            epoch_height: 0,
        }
    }

    #[test]
    #[should_panic(expected = "ERR_INVALID_ORACLE_ADDRESS")]
    fn ri_not_oracle() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            None,
        );
        contract.request_ft_transfer(
            token(),
            100,
            alice()
        );
    }

    #[test]
    fn ri_create_dr_success() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            None,
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }


    #[test]
    fn ri_whitelisted_success() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"alice.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }

    #[test]
    #[should_panic(expected = "ERR_NOT_WHITELISTED")]
    fn ri_unwhitelisted_fail() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"bob.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }

    #[test]
    fn ri_empty_tags_nonce_works() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"alice.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });

        assert!(contract.data_requests.get(&1).is_some());
    }

    #[test]
    fn ri_some_tags_nonce_works() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"alice.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(vec!["butt".to_owned(),"on".to_owned()]),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });

        assert!(contract.data_requests.get(&1).is_some());
    }

    #[test]
    fn ri_nonce_iterates_properly() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let mut contract = RequestorContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"alice.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Some(Vec::new()),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            description: Some("a".to_string()),
            tags: Some(Vec::new()),
            data_type: DataRequestDataType::String,
            creator: alice(),
        });

        assert!(contract.data_requests.get(&2).is_some());
    }
}
