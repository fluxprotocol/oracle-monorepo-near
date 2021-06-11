use crate::*;
use near_sdk::{PromiseOrValue, ext_contract};

#[ext_contract(ext_self)]
trait SelfExt {
    fn proceed_dr_new(&mut self, sender: AccountId, amount: Balance, payload: NewDataRequestArgs);
}

#[near_bindgen]
impl Contract {

    pub fn requestor_get_tvl(&self, _contract_id: AccountId) -> PromiseOrValue<U128> {
        PromiseOrValue::Value(1.into())
    }
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
        PromiseOrValue::Value(U128(self.dr_new(sender.clone(), amount.into(), 0, payload)))
    }
}
