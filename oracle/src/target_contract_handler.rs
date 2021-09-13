// use crate::*;
// use near_sdk::json_types::{ U64 };
// use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
// use near_sdk::serde::{ Deserialize, Serialize };
// use near_sdk::{ AccountId, Gas, ext_contract, Promise };
// use types::Outcome;

#[ext_contract]
pub trait TargetContractExtern {
    fn set_outcome(request_id: U64, requester: AccountId, outcome: Outcome, tags: Option<Vec<String>>, final_arbitrator_triggered: bool);
}

// #[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)]
// pub struct TargetContract(pub AccountId);

const GAS_BASE_SET_OUTCOME: Gas = 250_000_000_000_000;

// impl TargetContract {
    pub fn set_outcome(
        &self,
        request_id: U64,
        requester: AccountId,
        outcome: Outcome,
        tags: Option<Vec<String>>,
        final_arbitrator_triggered: bool
    ) -> Promise {
        target_contract_extern::set_outcome(
            request_id,
            requester,
            outcome,
            tags,
            final_arbitrator_triggered,

            // NEAR params
            &self.0,
            1, 
            GAS_BASE_SET_OUTCOME / 10,
        )
    }
// }
