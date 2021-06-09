use crate::*;
use near_sdk::json_types::{ U64 };
use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::{ AccountId, Gas, ext_contract, Promise };
use data_request::Outcome;

#[ext_contract]
pub trait TargetContractExtern {
    fn set_outcome(request_id: U64, requestor: AccountId, outcome: Outcome, tags: Option<Vec<String>>);
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct TargetContract(pub AccountId);

const GAS_BASE_SET_OUTCOME: Gas = 5_000_000_000_000;

impl TargetContract {
    pub fn set_outcome(&self, request_id: U64, requestor: AccountId, outcome: data_request::Outcome, tags: Option<Vec<String>>) -> Promise {
        target_contract_extern::set_outcome(
            request_id,
            requestor,
            outcome,
            tags,

            // NEAR params
            &self.0,
            0, 
            GAS_BASE_SET_OUTCOME,
        )
    }
}
