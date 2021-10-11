use near_sdk::{
    AccountId,
    Gas,
    Promise,
    json_types::{
        U128,
    },
    ext_contract,
};

#[ext_contract]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_balance_of(&self, account_id: AccountId);
}

const GAS_BASE_TRANSFER: Gas = 5_000_000_000_000;

pub fn fungible_token_transfer(token_account_id: AccountId, receiver_id: AccountId, value: u128) -> Promise {
    // AUDIT: When calling this without a callback, you need to be sure the storage is registered
    //     for the receiver. Otherwise the transfer will fail and the funds will be returned to this
    //     contract.
    fungible_token::ft_transfer(
        receiver_id,
        U128(value),
        None,

        // NEAR params
        &token_account_id,
        1,
        GAS_BASE_TRANSFER
    )
}
