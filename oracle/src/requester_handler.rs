use crate::*;
use flux_sdk::{
    consts::GAS_BASE_SET_OUTCOME, data_request::NewDataRequestArgs, outcome::Outcome,
    requester::Requester, types::WrappedBalance,
};
use near_sdk::{ext_contract, Promise, PromiseOrValue};

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
            code_base_url: None,
        }
    }
    fn set_outcome(&self, outcome: Outcome, tags: Vec<String>) -> Promise {
        // AUDIT: Suggestions:
        //     - `1` yoctoNEAR is not necessary, since this callback can only be received from the oracle and not from the user.
        //     - Gas limit is a bit tight. Ideally there is larger amount of gas that can be configured.
        // SOLUTION:
        //     - remove 1 yoctoNEAR
        //     - Figure out how to get ideal gas amount and implement
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
        payload: NewDataRequestArgs,
    ) -> PromiseOrValue<WrappedBalance> {
        PromiseOrValue::Value(U128(self.dr_new(sender.clone(), amount.into(), payload)))
    }
}
