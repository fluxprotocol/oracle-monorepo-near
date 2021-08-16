use near_sdk::{env, near_bindgen, AccountId, Balance, Promise};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{ Deserialize, Serialize };
use near_sdk::serde_json::json;
use near_sdk::json_types::{U64, U128, ValidAccountId};
use near_sdk::collections::UnorderedSet;
use fungible_token_handler::fungible_token_transfer_call;

mod fungible_token_handler;

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
pub struct Source {
    pub end_point: String,
    pub source_path: String
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq)]
pub enum DataRequestDataType {
    Number(U128),
    String,
}

#[derive(Serialize, Deserialize)]
pub struct NewDataRequestArgs {
    pub sources: Vec<Source>,
    pub tags: Option<Vec<String>>,
    pub description: Option<String>,
    pub outcomes: Option<Vec<String>>,
    pub challenge_period: WrappedTimestamp,
    pub settlement_time: WrappedTimestamp,
    pub target_contract: AccountId,
    pub data_type: DataRequestDataType,
    pub creator: AccountId,
}

near_sdk::setup_alloc!();

pub type WrappedTimestamp = U64;
pub type WrappedBalance = U128;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct RequestInterfaceContract {
    pub oracle: AccountId,
    pub stake_token: AccountId,
    pub whitelist: UnorderedSet<AccountId> // accounts allowed to call create_data_request(). if len() == 0, no whitelist (any account can make data request)
}

impl Default for RequestInterfaceContract {
    fn default() -> Self {
        env::panic(b"Contract should be initialized before usage")
    }
}

// Private methods
impl RequestInterfaceContract {
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
}

#[near_bindgen]
impl RequestInterfaceContract {
    #[init]
    pub fn new(
        oracle: AccountId,
        stake_token: AccountId,
        whitelist: Option<Vec<ValidAccountId>>,
    ) -> Self {
        let mut this = Self {
            oracle,
            stake_token,
            whitelist: UnorderedSet::new(b"".to_vec())
        };

        // populate whitelist
        if let Some(whitelist) = whitelist {
            for acct in whitelist {
                this.whitelist.insert(acct.as_ref());
            }
        }

        this
    }

    /**
     * @notice creates a new data request on the oracle (must be whitelisted on oracle first)
     * @returns ID of data request
     */
    pub fn create_data_request(
        &self,
        amount: WrappedBalance,
        payload: NewDataRequestArgs
    ) -> Promise {
        self.assert_whitelisted();
        fungible_token_transfer_call(
            self.stake_token.clone(),
            self.oracle.clone(),
            amount.into(),
            json!({"NewDataRequest": payload}).to_string() 
        )
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

    fn target() -> AccountId {
        "target.near".to_string()
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
        let contract = RequestInterfaceContract::new(
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
        let contract = RequestInterfaceContract::new(
            oracle(),
            token(),
            None,
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }


    #[test]
    fn ri_whitelisted_success() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let contract = RequestInterfaceContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"alice.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }

    #[test]
    #[should_panic(expected = "ERR_NOT_WHITELISTED")]
    fn ri_unwhitelisted_fail() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let contract = RequestInterfaceContract::new(
            oracle(),
            token(),
            Some(vec![serde_json::from_str("\"bob.near\"").unwrap()])
        );

        contract.create_data_request(U128(100), NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: DataRequestDataType::String,
            creator: alice(),
        });
    }
}
