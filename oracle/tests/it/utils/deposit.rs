use crate::utils::*;
use flux_sdk::consts::MAX_GAS;

pub fn storage_deposit(
    receiver: &str,
    sender: &UserAccount,
    deposit: u128,
    to_register: Option<AccountId>,
) {
    let res = sender.call(
        receiver.to_string(),
        "storage_deposit",
        json!({ "account_id": to_register }).to_string().as_bytes(),
        MAX_GAS,
        deposit,
    );

    assert!(res.is_ok(), "storage deposit failed with res: {:?}", res);
}

pub fn near_deposit(sender: &UserAccount, deposit: u128) {
    let res = sender.call(
        TOKEN_CONTRACT_ID.to_string(),
        "near_deposit",
        json!({}).to_string().as_bytes(),
        MAX_GAS,
        deposit,
    );
    assert!(res.is_ok(), "wnear deposit failed with res: {:?}", res);
}
