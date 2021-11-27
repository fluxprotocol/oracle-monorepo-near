use crate::utils::*;
use flux_sdk::{
    consts::PERCENTAGE_DIVISOR,
    outcome::{AnswerType, Outcome},
};

#[test]
fn dr_claim_flow() {
    let stake_amount = to_yocto("250");
    let stake_cost = 200;
    let validity_bond = 1;
    let fee = 5;

    let init_res = TestUtils::init(None);
    let init_balance_alice = init_res.alice.get_token_balance(None);

    let _res = init_res.alice.dr_new(fee, None);
    let post_new_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));

    assert_eq!(post_new_balance_oracle, validity_bond + fee);

    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    let outcome = Outcome::Answer(AnswerType::String("test".to_string()));
    let _res = init_res.alice.stake(0, outcome, stake_amount);
    println!("response {:?}", _res);

    let _post_stake_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));
    let post_stake_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(
        post_stake_balance_alice,
        init_balance_alice - stake_cost - validity_bond - fee
    );

    init_res.bob.ft_transfer(&REQUESTER_CONTRACT_ID, 1_000_000);
    init_res.alice.finalize(0);
    init_res.alice.claim(0);

    let post_claim_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(post_claim_balance_alice, init_balance_alice - validity_bond); // expect initial balance - validity bond since refunds are handled by requester contract
}

#[test]
fn dr_fixed_fee_flow() {
    let custom_fee_amount = 100;
    let stake_amount = to_yocto("250");
    let dr_cost = 1 + custom_fee_amount;

    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond: 1,
        min_resolution_bond: 1,
        final_arbitrator_invoke_amount: 2500,
    }));

    let init_balance_alice = init_res.alice.get_token_balance(None);

    let _res = init_res.alice.dr_new(custom_fee_amount, Some(1));
    let _post_new_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));

    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    let outcome = Outcome::Answer(AnswerType::String("test".to_string()));
    let _res = init_res.alice.stake(0, outcome, stake_amount);

    println!("res {:?}", _res);

    let _post_stake_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));
    let post_stake_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(
        post_stake_balance_alice,
        init_balance_alice - dr_cost - custom_fee_amount * 2
    );

    init_res.bob.ft_transfer(&REQUESTER_CONTRACT_ID, 100_000);

    init_res.alice.finalize(0);
    init_res.alice.claim(0);

    let post_claim_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(post_claim_balance_alice, init_balance_alice - 1); // expect initial balance - validity bond since refunds are handled by requester contract
}

#[test]
fn dr_multiplier_flow() {
    let stake_cost = 200;
    let multiplier_amount = 10500_u16; // 105%
    let stake_amount = to_yocto("250");
    let dr_cost = 101;
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: Some(multiplier_amount),
        validity_bond: 1,
        min_resolution_bond: 1,
        final_arbitrator_invoke_amount: 2500,
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);

    let _res = init_res.alice.dr_new(100, Some(1));
    let _post_new_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));

    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    let outcome = Outcome::Answer(AnswerType::String("test".to_string()));
    let _res = init_res.alice.stake(0, outcome, stake_amount);

    let _post_stake_balance_oracle = init_res
        .alice
        .get_token_balance(Some(ORACLE_CONTRACT_ID.to_string()));
    let post_stake_balance_alice = init_res.alice.get_token_balance(None);
    let weighted_stake_cost =
        u128::from(stake_cost as u64 * multiplier_amount as u64 / PERCENTAGE_DIVISOR as u64);
    assert_eq!(
        post_stake_balance_alice,
        init_balance_alice - dr_cost - weighted_stake_cost
    );

    init_res.bob.ft_transfer(&REQUESTER_CONTRACT_ID, 100_000);

    init_res.alice.finalize(0);
    init_res.alice.claim(0);

    let post_claim_balance_alice = init_res.alice.get_token_balance(None);
    assert_eq!(post_claim_balance_alice, init_balance_alice - 1); // expect initial balance - validity bond since refunds are handled by requester contract
}
