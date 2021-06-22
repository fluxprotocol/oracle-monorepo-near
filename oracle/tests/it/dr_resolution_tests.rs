use crate::utils::*;
use oracle::data_request::AnswerType;

#[test]
fn dr_resolution_flow_test() {
    let stake_amount = to_yocto("250"); 
    let stake_cost = 200;
    let dr_cost = 100;
    let init_res = TestUtils::init();
    let init_balance_alice = init_res.alice.get_token_balance(None);

    let _res = init_res.alice.dr_new();
    let _post_new_balance_oracle = init_res.alice.get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));
    
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    let outcome = data_request::Outcome::Answer(AnswerType::String("test".to_string()));
    let _res = init_res.alice.stake(0, outcome, stake_amount);

    let _post_stake_balance_oracle = init_res.alice.get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));
    let post_stake_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(post_stake_balance_alice, init_balance_alice - stake_cost - dr_cost);
    
    init_res.alice.finalize(0);
    init_res.alice.claim(0);
    
    let post_claim_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(post_claim_balance_alice, init_balance_alice);
}
