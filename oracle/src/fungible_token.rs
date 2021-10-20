use near_sdk::{ext_contract, json_types::U128, AccountId, Promise};

use flux_sdk::consts::GAS_BASE_TRANSFER;

#[ext_contract]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_balance_of(&self, account_id: AccountId);
}

pub fn fungible_token_transfer(
    token_account_id: AccountId,
    receiver_id: AccountId,
    value: u128,
) -> Promise {
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
        GAS_BASE_TRANSFER,
    )
}
