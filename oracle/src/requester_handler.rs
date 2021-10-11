use crate::*;
use near_sdk::{
    PromiseOrValue,
    ext_contract, 
    Gas,
    Promise
};
use flux_sdk::{
    data_request::NewDataRequestArgs,
    outcome::Outcome,
    types::WrappedBalance,
    requester::Requester
};

const GAS_BASE_SET_OUTCOME: Gas = 250_000_000_000_000;

#[ext_contract]
pub trait RequesterContractExtern {
    fn set_outcome(requester: AccountId, outcome: Outcome, tags: Vec<String>);
}

#[ext_contract(ext_self)]
trait SelfExt {
    fn proceed_dr_new(&mut self, sender: AccountId, amount: Balance, payload: NewDataRequestArgs);
}

pub trait RequesterHandler {
    fn new_no_whitelist(account_id: &AccountId) -> Self;
    fn set_outcome(&self, outcome: Outcome, tags: Vec<String>) -> Promise;
}

impl RequesterHandler for Requester {
    fn new_no_whitelist(account_id: &AccountId) -> Self {
        Self {
            contract_name: "".to_string(),
            account_id: account_id.to_string(),
            stake_multiplier: None,
            code_base_url: None
        }
    }
    fn set_outcome(
        &self,
        outcome: Outcome,
        tags: Vec<String>
    ) -> Promise {
        requester_contract_extern::set_outcome(
            self.account_id.to_string(),
            outcome,
            tags,

            // NEAR params
            &self.account_id,
            1, 
            GAS_BASE_SET_OUTCOME / 10,
        )
    }
}

#[near_bindgen]
impl Contract {
    /**
     * @notice called in ft_on_transfer to chain together fetching of TVL and data request creation
     */
    #[private]
    pub fn ft_dr_new_callback(
        &mut self,
        sender: AccountId,
        amount: Balance,
        payload: NewDataRequestArgs
    ) -> PromiseOrValue<WrappedBalance> {
        PromiseOrValue::Value(U128(self.dr_new(sender.clone(), amount.into(), payload)))
    }
}
