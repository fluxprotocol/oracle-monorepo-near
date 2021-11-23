use near_sdk::{env, AccountId, Balance, Promise, StorageUsage};
use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer.
    pub struct u256(4);
}

/*** operators that does not take decimals into account ***/
pub fn calc_product(a: u128, b: u128, divisor: u128) -> u128 {
    let a_u256 = u256::from(a);
    let b_u256 = u256::from(b);
    let divisor_u256 = u256::from(divisor);

    (a_u256 * b_u256 / divisor_u256).as_u128()
}

pub fn refund_storage(initial_storage: StorageUsage, sender_id: AccountId) {
    let current_storage = env::storage_usage();
    let attached_deposit = env::attached_deposit();
    let refund_amount = if current_storage > initial_storage {
        let required_deposit =
            Balance::from(current_storage - initial_storage) * env::STORAGE_PRICE_PER_BYTE;
        assert!(
            required_deposit <= attached_deposit,
            "The required attached deposit is {}, but the given attached deposit is is {}",
            required_deposit,
            attached_deposit,
        );
        attached_deposit - required_deposit
    } else {
        attached_deposit + Balance::from(initial_storage - current_storage) * env::STORAGE_PRICE_PER_BYTE
    };
    if refund_amount > 0 {
        Promise::new(sender_id).transfer(refund_amount);
    }
}

pub fn ns_to_ms(ns_timestamp: u64) -> u64 {
    ns_timestamp / 1_000_000
}

pub fn multiply_stake(stake: u128, multiplier: Option<u16>) -> u128 {
    match multiplier {
        Some(m) => {
            let base = 10_000_u128;
            stake * m as u128 / base
        }
        None => stake,
    }
}