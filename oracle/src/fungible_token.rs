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

pub fn fungible_token_balance_of(token_account_id: AccountId, account_id: AccountId) -> Promise {
    fungible_token::ft_balance_of(
        account_id,
        &token_account_id,
        0,
        4_000_000_000_000
    )
}
