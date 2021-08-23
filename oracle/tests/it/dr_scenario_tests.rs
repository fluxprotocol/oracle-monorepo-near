use crate::utils::*;

// Scenario: Bob stakes correctly and Carol takes turns (incorrectly) disputing
// until the final arbitrator is invoked and Alice must finalize
#[test]
fn dr_scenario_1() {
    // configure test options and create data request
    let validity_bond = 1;
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond,
        final_arbitrator_invoke_amount: 2500
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);
    let init_balance_bob = init_res.bob.get_token_balance(None);
    let init_balance_carol = init_res.carol.get_token_balance(None);
    println!("Request interface before data request creation: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    let _new_dr_res = init_res.alice.dr_new(0, Some(validity_bond));
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    
    println!("Request interface before staking: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    // println!("Bob balance before staking:   {}", init_balance_bob); // same for carol
    
    for i in 0..12 {
        let bond_size = calc_bond_size(validity_bond, i, None); // stake 2, 4, 16, 32, ...
        // even numbers => Bob stakes on correct outcome
        // odd numbers => Carol stakes on incorrect outcome
        match i % 2 == 0 {
            true => {
                println!("Round {}, bond size: {}, staking correctly with Bob", i, bond_size);
                let pre_stake_balance_bob = init_res.bob.get_token_balance(None);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test".to_string()));
                let _res = init_res.bob.stake(0, outcome_to_stake, bond_size);
                let post_stake_balance_bob = init_res.bob.get_token_balance(None);
                // make sure no refund (bond size is exactly met)
                assert_eq!(post_stake_balance_bob, pre_stake_balance_bob - bond_size);
            },
            false => {
                println!("Round {}, bond size: {}, staking incorrectly with Carol", i, bond_size);
                let pre_stake_balance_carol = init_res.carol.get_token_balance(None);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test_wrong".to_string()));
                let _res = init_res.carol.stake(0, outcome_to_stake, bond_size);
                let post_stake_balance_carol = init_res.carol.get_token_balance(None);
                // make sure no refund (bond size is exactly met)
                assert_eq!(post_stake_balance_carol, pre_stake_balance_carol - bond_size);
            }
        };
        // println!("Request interface balance after stake: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    }
    
    // since final arbitrator is invoked, any stakes after this point will be fully refunded
    
    // get balances before finalization and claim and amount spent on staking
    let pre_claim_balance_bob = init_res.bob.get_token_balance(None);
    let pre_claim_balance_carol = init_res.carol.get_token_balance(None);
    let pre_claim_difference_bob = init_balance_bob - pre_claim_balance_bob;
    let pre_claim_difference_carol = init_balance_carol - pre_claim_balance_carol;
    println!("Bob pre-claim balance:    {}", pre_claim_balance_bob);
    println!("Carol pre-claim balance:  {}", pre_claim_balance_carol);
    println!("Bob has spent {} altogether on staking", pre_claim_difference_bob);
    println!("Carol has spent {} altogether on staking", pre_claim_difference_carol);
    
    // finalize
    println!("Request interface balance before claim: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    let pre_outcome = init_res.alice.get_outcome(0);
    println!("Outcome before finalize: {:?}", pre_outcome);
    let correct_outcome = Outcome::Answer(AnswerType::String("test".to_string()));
    init_res.alice.dr_final_arbitrator_finalize(0, correct_outcome);
    let post_outcome = init_res.alice.get_outcome(0);
    println!("Outcome after finalize: {:?}", post_outcome);

    // claim
    let _claim_res = init_res.bob.claim(0);
    
    // get balances and differences from after staking/before claiming and before staking
    let post_balance_alice = init_res.alice.get_token_balance(None);
    let post_balance_bob = init_res.bob.get_token_balance(None);
    let post_balance_carol = init_res.carol.get_token_balance(None);
    let post_stake_difference_bob = post_balance_bob - pre_claim_balance_bob;
    let post_stake_difference_carol = post_balance_carol - pre_claim_balance_carol;
    let post_total_difference_bob = post_balance_bob - init_balance_bob;
    let post_total_difference_carol = init_balance_carol - post_balance_carol;
    let post_total_difference_alice = init_balance_alice - post_balance_alice;
    
    println!("Alice final balance:             {}", post_balance_alice);
    println!("Bob final balance:               {}", post_balance_bob);
    println!("Carol final balance:             {}", post_balance_carol);
    println!("Request interface final balance: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    
    println!("Bob gained {} from claim for a total profit of {}", post_stake_difference_bob, post_total_difference_bob);
    println!("Carol gained {} from claim for a total loss of {}", post_stake_difference_carol, post_total_difference_carol);
    println!("Alice lost {} altogether from validity bond", post_total_difference_alice);
    
}

// Scenario: Bob, Carol, and Jasper work together fill each resolution window
// (1/4 Bob, 1/4 Carol, 1/2 Jasper) while Peter escalates with a wrong staked
// outcome
#[test]
fn dr_scenario_2() {
    // configure test options and create data request
    let validity_bond = 2;
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond,
        final_arbitrator_invoke_amount: 2500
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);
    let init_balance_bob = init_res.bob.get_token_balance(None);
    let init_balance_carol = init_res.carol.get_token_balance(None);
    let init_balance_jasper = init_res.jasper.get_token_balance(None);
    let init_balance_peter = init_res.peter.get_token_balance(None);
    let _new_dr_res = init_res.alice.dr_new(0, Some(2));
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    
    // println!("Bob balance before staking:    {}", init_balance_bob); // same for carol, ...
    
    for i in 0..7 {
        let bond_size = calc_bond_size(validity_bond, i, None); // stake 4, 16, 32, ...
        // even numbers => Bob, Carol, Jasper stake on correct outcome
        // odd numbers => Peter stakes on incorrect outcome
        match i % 2 == 0 {
            true => {
                let quarter_of_bond = bond_size / 4;
                println!(
                    "Round {}, bond size: {}, staking correctly with Bob({}), Carol({}), Jasper({})",
                    i,
                    bond_size,
                    quarter_of_bond,
                    quarter_of_bond,
                    (quarter_of_bond*2)
                );
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test".to_string()));
                let _res = init_res.bob.stake(0, outcome_to_stake.clone(), quarter_of_bond);
                let _res = init_res.carol.stake(0, outcome_to_stake.clone(), quarter_of_bond);
                let _res = init_res.jasper.stake(0, outcome_to_stake, quarter_of_bond*2);
            },
            false => {
                println!("Round {}, bond size: {}, staking incorrectly with Peter", i, bond_size);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test_wrong".to_string()));
                let _res = init_res.peter.stake(0, outcome_to_stake, bond_size);
            }
        };
    }
    
    // get balances before finalization and claim and amount spent on staking 
    let pre_claim_balance_bob = init_res.bob.get_token_balance(None);
    let pre_claim_balance_carol = init_res.carol.get_token_balance(None);
    let pre_claim_balance_jasper = init_res.jasper.get_token_balance(None);
    let pre_claim_balance_peter = init_res.peter.get_token_balance(None);
    let pre_claim_difference_bob = init_balance_bob - pre_claim_balance_bob;
    let pre_claim_difference_carol = init_balance_carol - pre_claim_balance_carol;
    let pre_claim_difference_jasper = init_balance_jasper - pre_claim_balance_jasper;
    let pre_claim_difference_peter = init_balance_peter - pre_claim_balance_peter;
    println!("Bob pre-claim balance:     {}", pre_claim_balance_bob);
    println!("Carol pre-claim balance:   {}", pre_claim_balance_carol);
    println!("Jasper pre-claim balance:  {}", pre_claim_balance_jasper);
    println!("Peter pre-claim balance:   {}", pre_claim_balance_peter);
    println!("Bob has spent {} altogether on staking", pre_claim_difference_bob);
    println!("Carol has spent {} altogether on staking", pre_claim_difference_carol);
    println!("Jasper has spent {} altogether on staking", pre_claim_difference_jasper);
    println!("Peter has spent {} altogether on staking", pre_claim_difference_peter);
    
    // finalize
    let pre_outcome = init_res.alice.get_outcome(0);
    println!("Outcome on target before finalize: {:?}", pre_outcome);
    init_res.treasurer.ft_transfer(&TARGET_CONTRACT_ID, 100_000);
    init_res.alice.finalize(0);
    let post_outcome = init_res.alice.get_outcome(0);
    println!("Outcome on target after finalize: {:?}", post_outcome);

    // claim
    init_res.bob.claim(0);
    init_res.carol.claim(0);
    init_res.jasper.claim(0);
    // init_res.peter.claim(0);
    
    // get balances and differences from after staking/before claiming and before staking
    let post_balance_alice = init_res.alice.get_token_balance(None);
    let post_balance_bob = init_res.bob.get_token_balance(None);
    let post_balance_carol = init_res.carol.get_token_balance(None);
    let post_balance_jasper = init_res.jasper.get_token_balance(None);
    let post_balance_peter = init_res.peter.get_token_balance(None);
    let post_stake_difference_bob = post_balance_bob - pre_claim_balance_bob;
    let post_stake_difference_carol = post_balance_carol - pre_claim_balance_carol;
    let post_stake_difference_jasper = post_balance_jasper - pre_claim_balance_jasper;
    let post_stake_difference_peter = pre_claim_balance_peter - post_balance_peter;
    let post_total_difference_bob = post_balance_bob - init_balance_bob;
    let post_total_difference_carol = post_balance_carol - init_balance_carol;
    let post_total_difference_jasper = post_balance_jasper - init_balance_jasper;
    let post_total_difference_peter = init_balance_peter - post_balance_peter;
    let post_total_difference_alice = init_balance_alice - post_balance_alice;

    println!("Alice final balance:  {}", post_balance_alice);
    println!("Bob final balance:    {}", post_balance_bob);
    println!("Carol final balance:  {}", post_balance_carol);
    println!("Jasper final balance: {}", post_balance_jasper);
    println!("Peter final balance:  {}", post_balance_peter);

    println!("Bob gained {} from claim for a total profit of {}", post_stake_difference_bob, post_total_difference_bob);
    println!("Carol gained {} from claim for a total profit of {}", post_stake_difference_carol, post_total_difference_carol);
    println!("Jasper gained {} from claim for a total profit of {}", post_stake_difference_jasper, post_total_difference_jasper);
    println!("Peter gained {} from claim for a total profit of {}", post_stake_difference_peter, post_total_difference_peter);
    println!("Alice lost {} altogether", post_total_difference_alice);

    
}

// Scenario: Peter, Illia, and Vitalik work together to fill each resolution
// window with the incorrect outcome while Bob, Carol, and Jasper escalate with
// the correct staked outcome, all with proportional amounts staked
#[test]
fn dr_scenario_3() {
    // configure test options and create data request
    let validity_bond = 3;
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond,
        final_arbitrator_invoke_amount: 2500
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);
    let init_balance_bob = init_res.bob.get_token_balance(None);
    let init_balance_carol = init_res.carol.get_token_balance(None);
    let init_balance_jasper = init_res.jasper.get_token_balance(None);
    let init_balance_peter = init_res.peter.get_token_balance(None);
    let init_balance_illia = init_res.peter.get_token_balance(None);
    let init_balance_vitalik = init_res.peter.get_token_balance(None);
    let _new_dr_res = init_res.alice.dr_new(0, Some(3));
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    
    // println!("Bob balance before staking:    {}", init_balance_bob); // same for carol, ...
    
    for i in 0..8 {
        let bond_size = calc_bond_size(validity_bond, i, None); // stake 3, 6, 27, ...
        let third_of_bond = bond_size / 3;
        // even numbers => Peter, Illia, Vitalik stake on incorrect outcome
        // odd numbers => Bob, Carol, Jasper stake on correct outcome
        match i % 2 == 0 {
            true => {
                println!(
                    "Round {}, bond size: {}, staking incorrectly with Peter({}), Illia({}), Vitalik({})",
                    i,
                    bond_size,
                    third_of_bond,
                    third_of_bond,
                    third_of_bond
                );
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test_wrong".to_string()));
                let _res = init_res.peter.stake(0, outcome_to_stake.clone(), third_of_bond);
                let _res = init_res.illia.stake(0, outcome_to_stake.clone(), third_of_bond);
                let _res = init_res.vitalik.stake(0, outcome_to_stake.clone(), third_of_bond);
            },
            false => {
                println!(
                    "Round {}, bond size: {}, staking correctly with Bob({}), Carol({}), Jasper({})",
                    i,
                    bond_size,
                    third_of_bond,
                    third_of_bond,
                    third_of_bond,
                );
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test".to_string()));
                let _res = init_res.bob.stake(0, outcome_to_stake.clone(), third_of_bond);
                let _res = init_res.carol.stake(0, outcome_to_stake.clone(), third_of_bond);
                let _res = init_res.jasper.stake(0, outcome_to_stake.clone(), third_of_bond);
            }
        };
    }
    
    // get balances before finalization and claim and amount spent on staking
    let pre_claim_balance_bob = init_res.bob.get_token_balance(None);
    let pre_claim_balance_carol = init_res.carol.get_token_balance(None);
    let pre_claim_balance_jasper = init_res.jasper.get_token_balance(None);
    let pre_claim_balance_peter = init_res.peter.get_token_balance(None);
    let pre_claim_balance_illia = init_res.illia.get_token_balance(None);
    let pre_claim_balance_vitalik = init_res.vitalik.get_token_balance(None);
    let pre_claim_difference_bob = init_balance_bob - pre_claim_balance_bob;
    let pre_claim_difference_carol = init_balance_carol - pre_claim_balance_carol;
    let pre_claim_difference_jasper = init_balance_jasper - pre_claim_balance_jasper;
    let pre_claim_difference_peter = init_balance_peter - pre_claim_balance_peter;
    let pre_claim_difference_illia = init_balance_illia - pre_claim_balance_illia;
    let pre_claim_difference_vitalik = init_balance_vitalik - pre_claim_balance_vitalik;
    println!("Bob pre-claim balance:     {}", pre_claim_balance_bob);
    println!("Carol pre-claim balance:   {}", pre_claim_balance_carol);
    println!("Jasper pre-claim balance:  {}", pre_claim_balance_jasper);
    println!("Peter pre-claim balance:   {}", pre_claim_balance_peter);
    println!("Illia pre-claim balance:   {}", pre_claim_balance_illia);
    println!("Vitalik pre-claim balance: {}", pre_claim_balance_vitalik);
    println!("Bob has spent {} altogether on staking", pre_claim_difference_bob);
    println!("Carol has spent {} altogether on staking", pre_claim_difference_carol);
    println!("Jasper has spent {} altogether on staking", pre_claim_difference_jasper);
    println!("Peter has spent {} altogether on staking", pre_claim_difference_peter);
    println!("Illia has spent {} altogether on staking", pre_claim_difference_illia);
    println!("Vitalik has spent {} altogether on staking", pre_claim_difference_vitalik);
    
    init_res.treasurer.ft_transfer(&TARGET_CONTRACT_ID, 100_000);
    init_res.alice.finalize(0);
    init_res.bob.claim(0);
    init_res.carol.claim(0);
    init_res.jasper.claim(0);
    // init_res.peter.claim(0);
    // init_res.illia.claim(0);
    // init_res.vitalik.claim(0);
    
    // get balances and differences from after staking/before claiming and before staking
    let post_balance_alice = init_res.alice.get_token_balance(None);
    let post_balance_bob = init_res.bob.get_token_balance(None);
    let post_balance_carol = init_res.carol.get_token_balance(None);
    let post_balance_jasper = init_res.jasper.get_token_balance(None);
    let post_balance_peter = init_res.peter.get_token_balance(None);
    let post_balance_illia = init_res.illia.get_token_balance(None);
    let post_balance_vitalik = init_res.vitalik.get_token_balance(None);
    let post_stake_difference_bob = post_balance_bob - pre_claim_balance_bob;
    let post_stake_difference_carol = post_balance_carol - pre_claim_balance_carol;
    let post_stake_difference_jasper = post_balance_jasper - pre_claim_balance_jasper;
    let post_stake_difference_peter = pre_claim_balance_peter - post_balance_peter;
    let post_stake_difference_illia = pre_claim_balance_illia - post_balance_illia;
    let post_stake_difference_vitalik = pre_claim_balance_vitalik - post_balance_vitalik;
    let post_total_difference_bob = post_balance_bob - init_balance_bob;
    let post_total_difference_carol = post_balance_carol - init_balance_carol;
    let post_total_difference_jasper = post_balance_jasper - init_balance_jasper;
    let post_total_difference_peter = init_balance_peter - post_balance_peter;
    let post_total_difference_illia = init_balance_illia - post_balance_illia;
    let post_total_difference_vitalik = init_balance_vitalik - post_balance_vitalik;
    let post_total_difference_alice = init_balance_alice - post_balance_alice;

    println!("Alice final balance:   {}", post_balance_alice);
    println!("Bob final balance:     {}", post_balance_bob);
    println!("Carol final balance:   {}", post_balance_carol);
    println!("Jasper final balance:  {}", post_balance_jasper);
    println!("Illia final balance:   {}", post_balance_illia);
    println!("Vitalik final balance: {}", post_balance_vitalik);
    println!("Peter final balance:   {}", post_balance_peter);

    println!("Bob gained {} from claim for a total profit of {}", post_stake_difference_bob, post_total_difference_bob);
    println!("Carol gained {} from claim for a total profit of {}", post_stake_difference_carol, post_total_difference_carol);
    println!("Jasper gained {} from claim for a total profit of {}", post_stake_difference_jasper, post_total_difference_jasper);
    println!("Peter gained {} from claim for a total loss of {}", post_stake_difference_peter, post_total_difference_peter);
    println!("Illia gained {} from claim for a total loss of {}", post_stake_difference_illia, post_total_difference_illia);
    println!("Vitalik gained {} from claim for a total loss of {}", post_stake_difference_vitalik, post_total_difference_vitalik);
    println!("Alice lost {} altogether", post_total_difference_alice);
    
}

// Scenario: Bob stakes correctly and Carol takes turns escalating with
// incorrect outcome (similar to scenario 1) with a bond multiplier of 105%
#[test]
fn dr_scenario_multiplier() {
    // configure test options and create data request
    let validity_bond = 1;
    let multiplier = 10500; // 105%
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond,
        final_arbitrator_invoke_amount: 2500
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);
    let init_balance_bob = init_res.bob.get_token_balance(None);
    let init_balance_carol = init_res.carol.get_token_balance(None);
    let _new_dr_res = init_res.alice.dr_new(0, Some(1));
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    
    // println!("Bob balance before staking:   {}", init_balance_bob); // same for carol
    
    for i in 0..7 {
        let bond_size = calc_bond_size(validity_bond, i, Some(multiplier)); // stake 2, 4, 16, 33, ...
        // even numbers => Bob stakes on correct outcome
        // odd numbers => Carol stakes on incorrect outcome
        match i % 2 == 0 {
            true => {
                println!("Round {}, bond size: {}, staking correctly with Bob", i, bond_size);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test".to_string()));
                let _res = init_res.bob.stake(0, outcome_to_stake, bond_size);
            },
            false => {
                println!("Round {}, bond size: {}, staking incorrectly with Carol", i, bond_size);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test_wrong".to_string()));
                let _res = init_res.carol.stake(0, outcome_to_stake, bond_size);
            }
        };
    }
    
    // get balances before finalization and claim and amount spent on staking
    let pre_claim_balance_bob = init_res.bob.get_token_balance(None);
    let pre_claim_balance_carol = init_res.carol.get_token_balance(None);
    let pre_claim_difference_bob = init_balance_bob - pre_claim_balance_bob;
    let pre_claim_difference_carol = init_balance_carol - pre_claim_balance_carol;
    println!("Bob pre-claim balance:    {}", pre_claim_balance_bob);
    println!("Carol pre-claim balance:  {}", pre_claim_balance_carol);
    println!("Bob has spent {} altogether on staking", pre_claim_difference_bob);
    println!("Carol has spent {} altogether on staking", pre_claim_difference_carol);
    
    init_res.treasurer.ft_transfer(&TARGET_CONTRACT_ID, 100_000);
    init_res.alice.finalize(0);
    init_res.bob.claim(0);
    // init_res.carol.claim(0);
    
    // get balances and differences from after staking/before claiming and before staking
    let post_balance_alice = init_res.alice.get_token_balance(None);
    let post_balance_bob = init_res.bob.get_token_balance(None);
    let post_balance_carol = init_res.carol.get_token_balance(None);
    let post_stake_difference_bob = post_balance_bob - pre_claim_balance_bob;
    let post_stake_difference_carol = post_balance_carol - pre_claim_balance_carol;
    let post_total_difference_bob = post_balance_bob - init_balance_bob;
    let post_total_difference_carol = init_balance_carol - post_balance_carol;
    let post_total_difference_alice = init_balance_alice - post_balance_alice;

    println!("Alice final balance:  {}", post_balance_alice);
    println!("Bob final balance:    {}", post_balance_bob);
    println!("Carol final balance:  {}", post_balance_carol);

    println!("Bob gained {} from claim for a total profit of {}", post_stake_difference_bob, post_total_difference_bob);
    println!("Carol gained {} from claim for a total loss of {}", post_stake_difference_carol, post_total_difference_carol);
    println!("Alice lost {} altogether from validity bond", post_total_difference_alice);
    
}

// Scenario: Bob stakes correctly and Carol takes turns escalating with
// incorrect outcome (similar to scenario 1) with a fixed fee
#[test]
fn dr_scenario_fixed_fee() {
    // configure test options and create data request
    let validity_bond = 2;
    let init_res = TestUtils::init(Some(TestSetupArgs {
        stake_multiplier: None,
        validity_bond,
        final_arbitrator_invoke_amount: 2500
    }));
    let init_balance_alice = init_res.alice.get_token_balance(None);
    let init_balance_bob = init_res.bob.get_token_balance(None);
    let init_balance_carol = init_res.carol.get_token_balance(None);
    let _new_dr_res = init_res.alice.dr_new(100, Some(2));
    let dr_exist = init_res.alice.dr_exists(0);
    assert!(dr_exist, "something went wrong during dr creation");
    
    // println!("Bob balance before staking:   {}", init_balance_bob); // same for carol

    // send some funds to oracle so that is it able to pay out the fixed fee
    init_res.alice.ft_transfer(ORACLE_CONTRACT_ID, 9999);

    println!("Request interface before staking: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    
    for i in 0..7 {
        let bond_size = calc_bond_size(validity_bond, i, None); // stake 2, 4, 16, 33, ...
        // even numbers => Bob stakes on correct outcome
        // odd numbers => Carol stakes on incorrect outcome
        match i % 2 == 0 {
            true => {
                println!("Round {}, bond size: {}, staking correctly with Bob", i, bond_size);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test".to_string()));
                let _res = init_res.bob.stake(0, outcome_to_stake, bond_size);
            },
            false => {
                println!("Round {}, bond size: {}, staking incorrectly with Carol", i, bond_size);
                let outcome_to_stake = Outcome::Answer(AnswerType::String("test_wrong".to_string()));
                let _res = init_res.carol.stake(0, outcome_to_stake, bond_size);
            }
        };
    }

    println!("Request interface after staking: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));
    
    // get balances before finalization and claim and amount spent on staking
    let pre_claim_balance_bob = init_res.bob.get_token_balance(None);
    let pre_claim_balance_carol = init_res.carol.get_token_balance(None);
    let pre_claim_difference_bob = init_balance_bob - pre_claim_balance_bob;
    let pre_claim_difference_carol = init_balance_carol - pre_claim_balance_carol;
    println!("Bob pre-claim balance:    {}", pre_claim_balance_bob);
    println!("Carol pre-claim balance:  {}", pre_claim_balance_carol);
    println!("Bob has spent {} altogether on staking", pre_claim_difference_bob);
    println!("Carol has spent {} altogether on staking", pre_claim_difference_carol);
    
    init_res.treasurer.ft_transfer(&TARGET_CONTRACT_ID, 100_000);
    init_res.alice.finalize(0);
    init_res.bob.claim(0);
    // init_res.carol.claim(0);
    
    // get balances and differences from after staking/before claiming and before staking
    let post_balance_alice = init_res.alice.get_token_balance(None);
    let post_balance_bob = init_res.bob.get_token_balance(None);
    let post_balance_carol = init_res.carol.get_token_balance(None);
    let post_stake_difference_bob = post_balance_bob - pre_claim_balance_bob;
    let post_stake_difference_carol = post_balance_carol - pre_claim_balance_carol;
    let post_total_difference_bob = post_balance_bob - init_balance_bob;
    let post_total_difference_carol = init_balance_carol - post_balance_carol;
    let post_total_difference_alice = init_balance_alice - post_balance_alice;

    println!("Alice final balance:             {}", post_balance_alice);
    println!("Bob final balance:               {}", post_balance_bob);
    println!("Carol final balance:             {}", post_balance_carol);
    println!("Request interface final balance: {}", init_res.alice.get_token_balance(Some(REQUEST_INTERFACE_CONTRACT_ID.to_string())));

    println!("Bob gained {} from claim for a total profit of {}", post_stake_difference_bob, post_total_difference_bob);
    println!("Carol gained {} from claim for a total loss of {}", post_stake_difference_carol, post_total_difference_carol);
    println!("Alice lost {} altogether", post_total_difference_alice);
    
}