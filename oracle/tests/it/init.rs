use crate::utils::*;

#[test]
fn test_initiation() {
    let _init_res = TestUtils::init(None);
}

#[test]
fn test_balances() {
    let init_res = TestUtils::init(None);
    let balance = init_res.alice.get_token_balance(None);
    assert_eq!(balance, init_balance() / 2);
}
