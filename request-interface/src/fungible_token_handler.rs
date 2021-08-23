use crate::*;

use near_sdk::{
    AccountId,
    Gas,
    Promise,
    json_types::U128,
    ext_contract,
};

#[ext_contract(fungible_token)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) -> Promise;
    fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> Promise;
    fn ft_balance_of(&self, account_id: AccountId) -> Promise;
}

const GAS_BASE_TRANSFER: Gas = 5_000_000_000_000;
const DR_NEW_GAS: Gas = 200_000_000_000_000;

pub fn fungible_token_transfer(token_account_id: AccountId, receiver_id: AccountId, value: u128) -> Promise {
    fungible_token::ft_transfer(
        receiver_id,
        U128(value),
        None,
        // Near params
        &token_account_id,
        1,
        GAS_BASE_TRANSFER
    )
}

pub fn fungible_token_transfer_call(token_account_id: AccountId, receiver_id: AccountId, value: u128, msg: String) -> Promise {
    fungible_token::ft_transfer_call(
        receiver_id,
        U128(value),
        None,
        msg,
        // Near params
        &token_account_id,
        1,
        DR_NEW_GAS
    )
}

#[near_bindgen]
impl RequestInterfaceContract {
    pub fn request_ft_transfer(
        &self,
        token_id: AccountId,
        amount: Balance,
        receiver_id: AccountId
    ) -> Promise {
        self.assert_oracle();
        assert_eq!(self.stake_token.clone(), token_id, "ERR_INVALID_STAKE_TOKEN");
        fungible_token_transfer(self.stake_token.clone(), receiver_id.clone(), amount)
    }
}
