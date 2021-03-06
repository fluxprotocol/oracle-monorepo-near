use crate::*;

use crate::{
    fungible_token::fungible_token_transfer, helpers::multiply_stake, logger,
    requester_handler::RequesterHandler, resolution_window::ResolutionWindowHandler,
};
use flux_sdk::{
    config::OracleConfig,
    consts::GAS_BASE_TRANSFER,
    data_request::{
        ActiveDataRequest, ActiveDataRequestSummary, ClaimRes, DataRequestConfig,
        DataRequestConfigSummary, DataRequestDataType, DataRequestSummary, FinalizedDataRequest,
        FinalizedDataRequestSummary, NewDataRequestArgs, StakeDataRequestArgs,
    },
    outcome::{AnswerType, Outcome},
    resolution_window::{ResolutionWindow, ResolutionWindowSummary, WindowStakeResult},
    types::WrappedBalance,
};
use near_sdk::{
    collections::Vector,
    env, ext_contract,
    json_types::{U128, U64},
    AccountId, Balance, Promise, PromiseOrValue,
};

#[ext_contract]
trait ExtSelf {
    fn dr_proceed_finalization(request_id: U64, sender: AccountId);
}

trait DataRequestMethods {
    fn unstake(
        &mut self,
        sender: AccountId,
        round: u16,
        outcome: Outcome,
        amount: Balance,
    ) -> Balance;
    fn get_config_id(&self) -> u64;
    fn log_update(&self);
    fn summarize(&self) -> DataRequestSummary;
}

impl DataRequestMethods for DataRequest {
    // @returns amount of tokens that didn't get staked
    fn unstake(
        &mut self,
        sender: AccountId,
        round: u16,
        outcome: Outcome,
        amount: Balance,
    ) -> Balance {
        let resolution_windows = match self {
            DataRequest::Active(dr) => &dr.resolution_windows,
            DataRequest::Finalized(dr) => &dr.resolution_windows,
        };

        let mut window = resolution_windows
            .get(round as u64)
            .expect("ERR_NO_RESOLUTION_WINDOW");

        window.unstake(sender, outcome, amount)
    }

    fn get_config_id(&self) -> u64 {
        match self {
            DataRequest::Active(dr) => dr.global_config_id,
            DataRequest::Finalized(dr) => dr.global_config_id,
        }
    }

    fn log_update(&self) {
        match self {
            DataRequest::Active(dr) => logger::log_update_active_data_request(&dr),
            DataRequest::Finalized(dr) => logger::log_update_finalized_data_request(&dr),
        }
    }

    fn summarize(&self) -> DataRequestSummary {
        match self {
            DataRequest::Active(d) => DataRequestSummary::Active(d.summarize_dr()),
            DataRequest::Finalized(d) => DataRequestSummary::Finalized(d.summarize_dr()),
        }
    }
}

trait ActiveDataRequestChange {
    fn new(
        requester: Requester,
        id: u64,
        global_config_id: u64,
        global_config: &OracleConfig,
        paid_fee: Balance,
        request_data: NewDataRequestArgs,
    ) -> Self;
    fn stake(&mut self, sender: AccountId, outcome: Outcome, amount: Balance) -> Balance;
    fn invoke_final_arbitrator(&mut self, bond_size: Balance) -> bool;
    fn get_final_outcome(&self) -> Outcome;
}

impl ActiveDataRequestChange for ActiveDataRequest {
    fn new(
        requester: Requester,
        id: u64,
        global_config_id: u64,
        config: &OracleConfig,
        paid_fee: Balance,
        request_data: NewDataRequestArgs,
    ) -> Self {
        let resolution_windows = Vector::new(format!("rw{}", id).as_bytes().to_vec());

        Self {
            id,
            sources: request_data.sources.unwrap(),
            outcomes: request_data.outcomes,
            requester: requester.clone(),
            resolution_windows,
            global_config_id,
            request_config: DataRequestConfig {
                default_challenge_window_duration: config.default_challenge_window_duration.into(),
                final_arbitrator_invoke_amount: config.final_arbitrator_invoke_amount.into(),
                final_arbitrator: config.final_arbitrator.to_string(),
                validity_bond: config.validity_bond.into(),
                stake_multiplier: requester.stake_multiplier,
                paid_fee,
                min_resolution_bond: config.min_resolution_bond.into(),
            },
            initial_challenge_period: request_data.challenge_period.into(),
            final_arbitrator_triggered: false,
            description: request_data.description,
            tags: request_data.tags,
            data_type: request_data.data_type,
            provider: request_data.provider,
        }
    }

    // @returns amount of tokens that didn't get staked
    fn stake(&mut self, sender: AccountId, outcome: Outcome, amount: Balance) -> Balance {
        let mut window: ResolutionWindow = match self.resolution_windows.len() {
            0 => ResolutionWindowHandler::new(
                self.id,
                0,
                self.calc_resolution_bond(),
                self.initial_challenge_period,
                env::block_timestamp(),
            ),
            _ => self
                .resolution_windows
                .get(self.resolution_windows.len() - 1)
                .unwrap(),
        };

        let unspent = window.stake(sender, outcome, amount);

        // If first window push it to vec, else replace updated window struct
        if self.resolution_windows.len() == 0 {
            self.resolution_windows.push(&window);
        } else {
            self.resolution_windows.replace(
                self.resolution_windows.len() - 1, // Last window
                &window,
            );
        }

        // Check if this stake is bonded for the current window and if the final arbitrator should be invoked.
        // If the final arbitrator is invoked other stake won't come through.
        if window.bonded_outcome.is_some() && !self.invoke_final_arbitrator(window.bond_size) {
            
            // First challenge window should have initial_challenge_period 
            let duration = match self.resolution_windows.len() {
                1 => self.initial_challenge_period,
                _ => self.request_config.default_challenge_window_duration,
            };

            self.resolution_windows.push(&ResolutionWindowHandler::new(
                self.id,
                self.resolution_windows.len() as u16,
                window.bond_size,
                duration,
                env::block_timestamp(),
            ));
        }

        unspent
    }

    // @returns whether final arbitrator was triggered
    fn invoke_final_arbitrator(&mut self, bond_size: Balance) -> bool {
        let should_invoke = bond_size >= self.request_config.final_arbitrator_invoke_amount;
        if should_invoke {
            self.final_arbitrator_triggered = true
        }
        self.final_arbitrator_triggered
    }

    fn get_final_outcome(&self) -> Outcome {
        assert!(
            self.resolution_windows.iter().count() >= 2,
            "No bonded outcome found or final arbitrator triggered after first round"
        );
        let last_bonded_window_i = self.resolution_windows.len() - 2; // Last window after end_time never has a bonded outcome
        let last_bonded_window = self.resolution_windows.get(last_bonded_window_i).unwrap();
        last_bonded_window
            .bonded_outcome
            .expect("Error, no final outcome")
    }
}

trait FinalizedDataRequestMethods {
    fn claim(&mut self, account_id: String) -> ClaimRes;
    fn summarize_dr(&self) -> FinalizedDataRequestSummary;
    fn finalize(&mut self, final_outcome: Outcome);
    fn return_validity_bond(
        &self,
        token: AccountId,
        requester: AccountId,
        validity_bond: u128,
    ) -> PromiseOrValue<bool>;
}

impl FinalizedDataRequestMethods for FinalizedDataRequest {
    /**
     * @notice Transforms a data request struct into another struct with Serde serialization
     */
    fn summarize_dr(&self) -> FinalizedDataRequestSummary {
        // format resolution windows inside this data request
        let mut resolution_windows = Vec::new();
        for i in self.resolution_windows.iter() {
            let rw = ResolutionWindowSummary {
                round: i.round,
                start_time: U64(i.start_time),
                end_time: U64(i.end_time),
                bond_size: U128(i.bond_size),
                bonded_outcome: i.bonded_outcome,
            };
            resolution_windows.push(rw);
        }

        // format data request
        FinalizedDataRequestSummary {
            id: self.id.into(),
            finalized_outcome: self.finalized_outcome.clone(),
            resolution_windows: resolution_windows,
            global_config_id: U64(self.global_config_id),
            paid_fee: U128(self.paid_fee),
        }
    }

    fn finalize(&mut self, final_outcome: Outcome) {
        self.finalized_outcome = final_outcome;
    }

    // @notice Return what's left of validity_bond to requester
    fn return_validity_bond(
        &self,
        token: AccountId,
        requester: AccountId,
        validity_bond: u128,
    ) -> PromiseOrValue<bool> {
        match self.finalized_outcome {
            Outcome::Answer(_) => {
                PromiseOrValue::Promise(fungible_token_transfer(token, requester, validity_bond))
            }
            Outcome::Invalid => PromiseOrValue::Value(false),
        }
    }

    fn claim(&mut self, account_id: String) -> ClaimRes {
        // Metrics for calculating payout
        let mut total_correct_staked = 0;
        let mut total_incorrect_staked = 0;
        let mut user_correct_stake = 0;

        // For any round after the resolution round handle generically
        // AUDIT: This may run out gas, if the number of windows is too large, because you iterate
        //     through all windows.
        // SOLUTION: See if more expensive to iterate through resolution windows than it is to
        // store aggregate of amount of stake for each user alongside resolution windows and amount they have staked in
        for round in 0..self.resolution_windows.len() {
            let mut window = self.resolution_windows.get(round).unwrap();
            let stake_state: WindowStakeResult =
                window.claim_for(account_id.to_string(), &self.finalized_outcome);
            match stake_state {
                WindowStakeResult::Correct(correctly_staked) => {
                    total_correct_staked += correctly_staked.bonded_stake;
                    user_correct_stake += correctly_staked.user_stake;
                }
                WindowStakeResult::Incorrect(incorrectly_staked) => {
                    total_incorrect_staked += incorrectly_staked
                }
                WindowStakeResult::NoResult => (),
            }

            self.resolution_windows.replace(round as u64, &window);
        }

        let stake_profit = match total_correct_staked {
            0 => 0,
            _ => helpers::calc_product(
                user_correct_stake,
                total_incorrect_staked,
                total_correct_staked,
            ),
        };

        let fee_profit = match total_correct_staked {
            0 => 0,
            _ => helpers::calc_product(user_correct_stake, self.paid_fee, total_correct_staked),
        };

        logger::log_claim(
            &account_id,
            self.id,
            total_correct_staked,
            total_incorrect_staked,
            user_correct_stake,
            stake_profit,
            fee_profit,
        );

        ClaimRes {
            payment_token_payout: fee_profit,
            stake_token_payout: user_correct_stake + stake_profit,
        }
    }
}

trait ActiveDataRequestView {
    fn assert_valid_outcome(&self, outcome: &Outcome);
    fn assert_valid_outcome_type(&self, outcome: &Outcome);
    fn assert_can_stake_on_outcome(&self, outcome: &Outcome);
    fn assert_can_finalize(&self);
    fn assert_final_arbitrator(&self);
    fn assert_final_arbitrator_invoked(&self);
    fn assert_final_arbitrator_not_invoked(&self);
    fn calc_resolution_bond(&self) -> Balance;
    fn summarize_dr(&self) -> ActiveDataRequestSummary;
}

impl ActiveDataRequestView for ActiveDataRequest {
    fn assert_valid_outcome(&self, outcome: &Outcome) {
        match &self.outcomes {
            Some(outcomes) => match outcome {
                Outcome::Answer(outcome) => {
                    // Only strings can be staked when an array of outcomes are set
                    match outcome {
                        AnswerType::String(string_answer) => {
                            assert!(outcomes.contains(&string_answer), "Incompatible outcome")
                        }
                        _ => panic!("ERR_OUTCOME_NOT_STRING"),
                    };
                }
                Outcome::Invalid => (),
            },
            None => (),
        }
    }

    fn assert_valid_outcome_type(&self, outcome: &Outcome) {
        match outcome {
            Outcome::Answer(answer) => match answer {
                AnswerType::String(_) => assert_eq!(
                    self.data_type,
                    DataRequestDataType::String,
                    "ERR_WRONG_OUTCOME_TYPE"
                ),
                AnswerType::Number(ans_num) => match self.data_type {
                    DataRequestDataType::Number(dr_multiplier) => {
                        assert_eq!(dr_multiplier, ans_num.multiplier, "ERR_WRONG_MULTIPLIER")
                    }
                    _ => panic!("ERR_WRONG_OUTCOME_TYPE"),
                },
            },
            _ => (),
        }
    }

    fn assert_can_stake_on_outcome(&self, outcome: &Outcome) {
        if self.resolution_windows.len() > 1 {
            let last_window = self
                .resolution_windows
                .get(self.resolution_windows.len() - 2)
                .unwrap();
            assert_ne!(
                &last_window.bonded_outcome.unwrap(),
                outcome,
                "Outcome is incompatible for this round"
            );
        }
    }

    fn assert_can_finalize(&self) {
        assert!(
            self.resolution_windows.len() > 0,
            "Error no bonded outcome, `DataRequest` still in progress"
        );
        let window = self
            .resolution_windows
            .get(self.resolution_windows.len() - 1)
            .unwrap();
        assert!(
            !self.final_arbitrator_triggered,
            "Can only be finalized by final arbitrator: {}",
            self.request_config.final_arbitrator
        );
        assert!(
            env::block_timestamp() >= window.end_time,
            "Error can only be finalized after final dispute round has timed out"
        );
    }

    fn assert_final_arbitrator(&self) {
        assert_eq!(
            self.request_config.final_arbitrator,
            env::predecessor_account_id(),
            "sender is not the final arbitrator of this `DataRequest`, the final arbitrator is: {}",
            self.request_config.final_arbitrator
        );
    }

    fn assert_final_arbitrator_invoked(&self) {
        assert!(
            self.final_arbitrator_triggered,
            "Final arbitrator can not finalize `DataRequest` with id: {}",
            self.id
        );
    }

    fn assert_final_arbitrator_not_invoked(&self) {
        assert!(
            !self.final_arbitrator_triggered,
            "Final arbitrator is invoked for `DataRequest` with id: {}",
            self.id
        );
    }

    /**
     * @notice Calculates the size of the resolution bond. If the accumulated fee is smaller than the validity bond, we payout the validity bond to validators, thus they have to stake double in order to be
     * eligible for the reward, in the case that the fee is greater than the validity bond validators need to have a cumulative stake of double the fee amount
     * @returns The size of the initial `resolution_bond` denominated in `stake_token`
     */
    fn calc_resolution_bond(&self) -> Balance {
        let base_bond = if self.request_config.paid_fee >= self.request_config.validity_bond {
            self.request_config.paid_fee
        } else {
            self.request_config.validity_bond
        };

        let res_bond = if base_bond < self.request_config.min_resolution_bond {
            self.request_config.min_resolution_bond
        } else {
            base_bond
        };

        multiply_stake(res_bond, self.request_config.stake_multiplier)
    }

    /**
     * @notice Transforms a data request struct into another struct with Serde serialization
     */
    fn summarize_dr(&self) -> ActiveDataRequestSummary {
        // format resolution windows inside this data request
        let mut resolution_windows = Vec::new();
        for i in self.resolution_windows.iter() {
            let rw = ResolutionWindowSummary {
                round: i.round,
                start_time: U64(i.start_time),
                end_time: U64(i.end_time),
                bond_size: U128(i.bond_size),
                bonded_outcome: i.bonded_outcome,
            };
            resolution_windows.push(rw);
        }

        // format data request
        ActiveDataRequestSummary {
            id: U64(self.id),
            description: self.description.clone(),
            sources: self.sources.clone(),
            outcomes: self.outcomes.clone(),
            requester: self.requester.clone(),
            resolution_windows: resolution_windows,
            global_config_id: U64(self.global_config_id),
            initial_challenge_period: U64(self.initial_challenge_period),
            final_arbitrator_triggered: self.final_arbitrator_triggered,
            tags: self.tags.clone(),
            data_type: self.data_type.clone(),
            request_config: DataRequestConfigSummary {
                validity_bond: U128(self.request_config.validity_bond),
                paid_fee: U128(self.request_config.paid_fee),
                stake_multiplier: self.request_config.stake_multiplier,
                min_resolution_bond: U128(self.request_config.min_resolution_bond),
            },
        }
    }
}

#[near_bindgen]
impl Contract {
    pub fn dr_exists(&self, id: U64) -> bool {
        self.data_requests.get(id.into()).is_some()
    }

    // Payable for potential future upgrade where storage is no longer subsidized
    #[payable]
    pub fn dr_new(
        &mut self,
        sender: AccountId,
        amount: Balance,
        payload: NewDataRequestArgs,
    ) -> Balance {
        self.assert_unpaused();
        let config = self.get_config();
        let validity_bond: u128 = config.validity_bond.into();
        self.assert_whitelisted(sender.to_string());
        self.assert_sender(&config.payment_token);
        assert!(
            amount >= validity_bond,
            "Validity bond of {} not reached, received only {}",
            validity_bond,
            amount
        );
        self.dr_validate(&payload);

        let paid_fee = amount - validity_bond;

        let requester = self.whitelist.whitelist_get_expect(&sender);
        let dr = ActiveDataRequest::new(
            requester,
            self.data_requests.len() as u64, // dr_id
            self.configs.len() - 1,          // dr's config id
            &config,
            paid_fee,
            payload,
        );

        logger::log_new_data_request(&dr);
        self.data_requests.push(&DataRequest::Active(dr));

        0
    }

    #[payable]
    pub fn dr_stake(
        &mut self,
        sender: AccountId,
        amount: Balance,
        payload: StakeDataRequestArgs,
    ) -> PromiseOrValue<WrappedBalance> {
        self.assert_unpaused();
        let mut dr = self.dr_get_expect_active(payload.id.into());
        let config = self.configs.get(dr.global_config_id).unwrap();
        self.assert_sender(&config.stake_token);
        dr.assert_final_arbitrator_not_invoked();
        dr.assert_can_stake_on_outcome(&payload.outcome);
        dr.assert_valid_outcome(&payload.outcome);
        dr.assert_valid_outcome_type(&payload.outcome);

        let unspent_stake = dr.stake(sender, payload.outcome, amount);
        logger::log_update_active_data_request(&dr);
        self.data_requests
            .replace(payload.id.into(), &DataRequest::Active(dr));

        PromiseOrValue::Value(U128(unspent_stake))
    }

    #[payable]
    pub fn dr_unstake(
        &mut self,
        request_id: U64,
        resolution_round: u16,
        outcome: Outcome,
        amount: U128,
    ) {
        self.assert_unpaused();
        let initial_storage = env::storage_usage();

        let mut dr = self.dr_get_expect(request_id.into());
        let unstaked = dr.unstake(
            env::predecessor_account_id(),
            resolution_round,
            outcome,
            amount.into(),
        );
        let config = self.configs.get(dr.get_config_id()).unwrap();

        helpers::refund_storage(initial_storage, env::predecessor_account_id());

        dr.log_update();
        fungible_token_transfer(config.stake_token, env::predecessor_account_id(), unstaked);
    }

    /**
     * @returns amount of tokens claimed
     */

    #[payable]
    pub fn dr_claim(&mut self, account_id: String, request_id: U64) -> Promise {
        self.assert_unpaused();
        let initial_storage = env::storage_usage();

        let mut dr = self.dr_get_expect_finalized(request_id.into());
        let stake_payout = dr.claim(account_id.to_string());
        let config = self.configs.get(dr.global_config_id).unwrap();

        logger::log_update_finalized_data_request(&dr);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());

        assert!(
            env::prepaid_gas() - env::used_gas() >= GAS_BASE_TRANSFER,
            "not enough gas for both token transfers"
        );

        // transfer owed stake tokens
        let prev_prom = if stake_payout.stake_token_payout > 0 {
            Some(fungible_token_transfer(
                config.stake_token,
                account_id.to_string(),
                stake_payout.stake_token_payout,
            ))
        } else {
            None
        };

        if stake_payout.payment_token_payout > 0 {
            // distribute fee + bond
            match prev_prom {
                Some(p) => p.then(fungible_token_transfer(
                    config.payment_token,
                    account_id,
                    stake_payout.payment_token_payout,
                )),
                None => fungible_token_transfer(
                    config.payment_token,
                    account_id,
                    stake_payout.payment_token_payout,
                ),
            }
        } else {
            match prev_prom {
                Some(p) => p,
                None => panic!("can't claim 0"),
            }
        }
    }

    pub fn dr_finalize(&mut self, request_id: U64) {
        self.assert_unpaused();
        let initial_storage = env::storage_usage();

        let dr = self.dr_get_expect_active(request_id.into());
        let requester = dr.requester.account_id.clone();
        let validity_bond = dr.request_config.validity_bond;
        dr.assert_can_finalize();
        let final_outcome = dr.get_final_outcome();

        dr.requester
            .set_outcome(final_outcome.clone(), dr.tags.clone());

        let config = self.configs.get(dr.global_config_id).unwrap();

        let fdr = self.trim_dr(dr, final_outcome);
        fdr.return_validity_bond(config.payment_token, requester, validity_bond);
        logger::log_update_finalized_data_request(&fdr);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());
        self.data_requests
            .replace(request_id.into(), &DataRequest::Finalized(fdr));
    }

    pub fn dr_finalize_by_provider(&mut self, request_id: U64, outcome: Outcome) {
        self.assert_unpaused();
        let initial_storage = env::storage_usage();
        let dr = self.dr_get_expect_active(request_id.into());
        dr.assert_valid_outcome(&outcome);

        match &dr.provider {
            Some(provider) => assert_eq!(
                provider,
                &env::predecessor_account_id(),
                "this request can only be finalized by the provider"
            ),
            None => panic!("error this is not a provider data request"),
        };

        dr.requester.set_outcome(outcome.clone(), dr.tags.clone());

        let fdr = self.trim_dr(dr, outcome);
        logger::log_update_finalized_data_request(&fdr);
        self.data_requests
            .replace(request_id.into(), &DataRequest::Finalized(fdr));

        helpers::refund_storage(initial_storage, env::predecessor_account_id());
    }

    pub fn dr_final_arbitrator_finalize(
        &mut self,
        request_id: U64,
        outcome: Outcome,
    ) -> PromiseOrValue<bool> {
        self.assert_unpaused();
        let initial_storage = env::storage_usage();

        let dr = self.dr_get_expect_active(request_id);
        let requester = dr.requester.account_id.clone();
        let validity_bond = dr.request_config.validity_bond;
        dr.assert_final_arbitrator();
        dr.assert_valid_outcome(&outcome);
        dr.assert_final_arbitrator_invoked();

        let config = self.configs.get(dr.global_config_id).unwrap();
        dr.requester.set_outcome(outcome.clone(), dr.tags.clone());
        let fdr = self.trim_dr(dr, outcome);

        logger::log_update_finalized_data_request(&fdr);
        let promise = fdr.return_validity_bond(config.payment_token, requester, validity_bond);

        self.data_requests
            .replace(request_id.into(), &DataRequest::Finalized(fdr));

        helpers::refund_storage(initial_storage, env::predecessor_account_id());
        promise
    }

    fn dr_get_expect(&self, id: U64) -> DataRequest {
        self.data_requests
            .get(id.into())
            .expect("ERR_DATA_REQUEST_NOT_FOUND")
    }

    fn dr_get_expect_active(&self, id: U64) -> ActiveDataRequest {
        match self
            .data_requests
            .get(id.into())
            .expect("Error no DataRequest with this id exists")
        {
            DataRequest::Active(dr) => dr,
            DataRequest::Finalized(_) => panic!("Error DataRequest is already finalized"),
        }
    }

    fn dr_get_expect_finalized(&self, id: U64) -> FinalizedDataRequest {
        match self
            .data_requests
            .get(id.into())
            .expect("Error no DataRequest with this id exists")
        {
            DataRequest::Active(_) => panic!("Error DataRequest is not yet finalized"),
            DataRequest::Finalized(dr) => dr,
        }
    }

    pub fn get_request_by_id(&self, id: U64) -> Option<DataRequestSummary> {
        let dr = self.data_requests.get(id.into());
        match dr {
            Some(d) => Some(d.summarize()),
            None => None,
        }
    }

    pub fn get_latest_request(&self) -> Option<DataRequestSummary> {
        if self.data_requests.len() < 1 {
            return None;
        }
        self.get_request_by_id(U64(self.data_requests.len() - 1))
    }

    pub fn get_outcome(&self, dr_id: U64) -> Outcome {
        self.dr_get_expect_finalized(dr_id.into()).finalized_outcome
    }

    pub fn get_requests(&self, from_index: U64, limit: U64) -> Vec<DataRequestSummary> {
        let i: u64 = from_index.into();
        (i..std::cmp::min(i + u64::from(limit), self.data_requests.len()))
            .map(|index| self.data_requests.get(index).unwrap().summarize())
            .collect()
    }
}

impl Contract {
    /**
     * @notice Transforms a data request struct into another struct with Serde serialization
     */
    fn trim_dr(&self, dr: ActiveDataRequest, finalized_outcome: Outcome) -> FinalizedDataRequest {
        // format data request
        FinalizedDataRequest {
            id: dr.id,
            finalized_outcome: finalized_outcome,
            resolution_windows: dr.resolution_windows,
            global_config_id: dr.global_config_id,
            paid_fee: dr.request_config.paid_fee,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use super::*;
    use flux_sdk::{
        config::{FeeConfig, OracleConfig},
        data_request::Source,
        outcome::AnswerType,
        requester::Requester,
        resolution_window::ResolutionWindow,
    };
    use near_sdk::{testing_env, MockedBlockchain, VMContext};

    fn alice() -> AccountId {
        "alice.near".to_string()
    }

    fn bob() -> AccountId {
        "bob.near".to_string()
    }

    fn carol() -> AccountId {
        "carol.near".to_string()
    }

    fn dave() -> AccountId {
        "dave.near".to_string()
    }

    fn token() -> AccountId {
        "token.near".to_string()
    }

    fn gov() -> AccountId {
        "gov.near".to_string()
    }

    fn sum_claim_res(claim_res: ClaimRes) -> u128 {
        claim_res.payment_token_payout + claim_res.stake_token_payout
    }

    fn registry_entry(account: AccountId) -> Requester {
        Requester {
            contract_name: account.clone(),
            account_id: account.clone(),
            stake_multiplier: None,
            code_base_url: None,
        }
    }

    fn finalize(contract: &mut Contract, dr_id: u64) -> &mut Contract {
        contract.dr_finalize(dr_id.into());
        contract
    }

    fn new_config(final_arb_amount: u128) -> OracleConfig {
        OracleConfig {
            gov: gov(),
            final_arbitrator: alice(),
            payment_token: token(),
            stake_token: token(),
            validity_bond: U128(100),
            max_outcomes: 8,
            default_challenge_window_duration: U64(1000),
            min_initial_challenge_window_duration: U64(1000),
            final_arbitrator_invoke_amount: U128(final_arb_amount),
            fee: FeeConfig {
                flux_market_cap: U128(50000),
                total_value_staked: U128(10000),
                resolution_fee_percentage: 10_000,
            },
            min_resolution_bond: U128(100),
        }
    }
    
    fn config() -> OracleConfig {
        new_config(1_000_000)
    }
    
    fn config_for_quick_final_arb() -> OracleConfig {
        new_config(250)
    }

    fn get_context(predecessor_account_id: AccountId) -> VMContext {
        VMContext {
            current_account_id: token(),
            signer_account_id: bob(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id,
            input: vec![],
            block_index: 0,
            block_timestamp: 0,
            account_balance: 10000 * 10u128.pow(24),
            account_locked_balance: 0,
            storage_usage: 10u64.pow(6),
            attached_deposit: 1000 * 10u128.pow(24),
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 0,
        }
    }

    #[test]
    #[should_panic(expected = "Invalid outcome list either exceeds min of: 2 or max of 8")]
    fn dr_new_single_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Err predecessor is not whitelisted")]
    fn dr_new_non_whitelisted() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(
            alice(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(0),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "This function can only be called by token.near")]
    fn dr_new_non_payment_token() {
        testing_env!(get_context(alice()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(0),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Too many sources provided, max sources is: 8")]
    fn dr_new_arg_source_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        let x1 = Source {
            end_point: "1".to_string(),
            source_path: "1".to_string(),
        };
        let x2 = Source {
            end_point: "2".to_string(),
            source_path: "2".to_string(),
        };
        let x3 = Source {
            end_point: "3".to_string(),
            source_path: "3".to_string(),
        };
        let x4 = Source {
            end_point: "4".to_string(),
            source_path: "4".to_string(),
        };
        let x5 = Source {
            end_point: "5".to_string(),
            source_path: "5".to_string(),
        };
        let x6 = Source {
            end_point: "6".to_string(),
            source_path: "6".to_string(),
        };
        let x7 = Source {
            end_point: "7".to_string(),
            source_path: "7".to_string(),
        };
        let x8 = Source {
            end_point: "8".to_string(),
            source_path: "8".to_string(),
        };
        let x9 = Source {
            end_point: "9".to_string(),
            source_path: "9".to_string(),
        };
        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(vec![x1, x2, x3, x4, x5, x6, x7, x8, x9]),
                outcomes: None,
                challenge_period: U64(1000),
                description: None,
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Invalid outcome list either exceeds min of: 2 or max of 8")]
    fn dr_new_arg_outcome_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec![
                    "1".to_string(),
                    "2".to_string(),
                    "3".to_string(),
                    "4".to_string(),
                    "5".to_string(),
                    "6".to_string(),
                    "7".to_string(),
                    "8".to_string(),
                    "9".to_string(),
                ]),
                challenge_period: U64(1000),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Description should be filled when no sources are given")]
    fn dr_description_required_no_sources() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(vec![]),
                outcomes: None,
                challenge_period: U64(1000),
                description: None,
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Challenge shorter than minimum challenge period")]
    fn dr_new_arg_challenge_period_below_min() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(999),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Challenge period exceeds maximum challenge period")]
    fn dr_new_arg_challenge_period_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(3001),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Validity bond of 100 not reached, received only 90")]
    fn dr_new_not_enough_amount() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            90,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    fn dr_new_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        let amount: Balance = contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
        assert_eq!(amount, 0);
    }

    fn dr_new(contract: &mut Contract) {
        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    #[should_panic(expected = "This function can only be called by token.near")]
    fn dr_stake_non_stake_token() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        testing_env!(get_context(alice()));
        contract.dr_stake(
            alice(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string())),
            },
        );
    }

    #[test]
    #[should_panic(expected = "Error no DataRequest with this id exists")]
    fn dr_stake_not_existing() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_stake(
            alice(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string())),
            },
        );
    }

    #[test]
    #[should_panic(expected = "Incompatible outcome")]
    fn dr_stake_incompatible_answer() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string())),
            },
        );
    }

    #[test]
    #[should_panic(expected = "Error DataRequest is already finalized")]
    fn dr_stake_finalized_market() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        let mut ct: VMContext = get_context(token());
        ct.block_timestamp = 1501;
        testing_env!(ct);

        let contract = finalize(&mut contract, 0);
        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
    }

    #[test]
    #[should_panic(expected = "Invalid outcome list either exceeds min of: 2 or max of 8")]
    fn dr_invalid_outcome_list() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
    }

    #[test]
    fn dr_stake_success_partial() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let _b = contract.dr_stake(
            alice(),
            5,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        // assert_eq!(b, 0, "Invalid balance");

        let request: ActiveDataRequest = contract.dr_get_expect_active(0.into());
        assert_eq!(request.resolution_windows.len(), 1);

        let round0: ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 1500);
        assert_eq!(round0.bond_size, 200);
    }

    #[test]
    fn dr_stake_success_full_at_t1() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let _b = contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        
        let _b = contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        let request: ActiveDataRequest = contract.dr_get_expect_active(0.into());
        assert_eq!(request.resolution_windows.len(), 3);

        let round0: ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 1500);
        assert_eq!(round0.bond_size, 200);

        let round1: ResolutionWindow = request.resolution_windows.get(1).unwrap();
        assert_eq!(round1.round, 1);
        assert_eq!(round1.end_time, 1500);
        assert_eq!(round1.bond_size, 400);
        
        let round2: ResolutionWindow = request.resolution_windows.get(2).unwrap();
        assert_eq!(round2.round, 2);
        assert_eq!(round2.end_time, 1000);
        assert_eq!(round2.bond_size, 800);
    }

    #[test]
    fn dr_stake_success_overstake_at_t600() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let mut ct: VMContext = get_context(token());
        ct.block_timestamp = 600;
        testing_env!(ct);

        let _b = contract.dr_stake(
            alice(),
            300,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        
        let _b = contract.dr_stake(
            alice(),
            800,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        // assert_eq!(b, 100, "Invalid balance");

        let request: ActiveDataRequest = contract.dr_get_expect_active(0.into());
        assert_eq!(request.resolution_windows.len(), 3);

        let round0: ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 2100);
        assert_eq!(round0.bond_size, 200);

        let round1: ResolutionWindow = request.resolution_windows.get(1).unwrap();
        assert_eq!(round1.round, 1);
        assert_eq!(round1.end_time, 2100);
        assert_eq!(round1.bond_size, 400);
        
        let round2: ResolutionWindow = request.resolution_windows.get(2).unwrap();
        assert_eq!(round2.round, 2);
        assert_eq!(round2.end_time, 1600);
        assert_eq!(round2.bond_size, 800);
    }

    #[test]
    #[should_panic(expected = "Can only be finalized by final arbitrator")]
    fn dr_finalize_final_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut c: OracleConfig = config();
        c.final_arbitrator_invoke_amount = U128(150);
        let mut contract = Contract::new(whitelist, c);
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        contract.dr_finalize(U64(0));
    }

    #[test]
    #[should_panic(expected = "Error no bonded outcome, `DataRequest` still in progress")]
    fn dr_finalize_no_resolutions() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        finalize(&mut contract, 0);
    }

    #[test]
    #[should_panic(
        expected = "Error can only be finalized after final dispute round has timed out"
    )]
    fn dr_finalize_active_challenge() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        contract.dr_finalize(U64(0));
    }

    #[test]
    fn dr_finalize_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        let mut ct: VMContext = get_context(token());
        ct.block_timestamp = 1501;
        testing_env!(ct);

        let contract = finalize(&mut contract, 0);

        let request: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        assert_eq!(request.resolution_windows.len(), 2);
        assert_eq!(
            request.finalized_outcome,
            data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        );
    }

    #[test]
    #[should_panic(expected = "Outcome is incompatible for this round")]
    fn dr_stake_same_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            300,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        contract.dr_stake(
            alice(),
            500,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
    }

    fn dr_finalize(contract: &mut Contract, outcome: Outcome) {
        contract.dr_stake(
            alice(),
            2000,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: outcome,
            },
        );

        let mut ct: VMContext = get_context(token());
        ct.block_timestamp = 1501;
        testing_env!(ct);

        finalize(contract, 0);
    }

    #[test]
    #[should_panic(expected = "ERR_DATA_REQUEST_NOT_FOUND")]
    fn dr_unstake_invalid_id() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_unstake(
            U64(0),
            0,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            U128(0),
        );
    }

    #[test]
    #[should_panic(expected = "Cannot withdraw from bonded outcome")]
    fn dr_unstake_bonded_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        contract.dr_unstake(
            U64(0),
            0,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            U128(0),
        );
    }

    #[test]
    #[should_panic(expected = "token.near has less staked on this outcome (0) than unstake amount")]
    fn dr_unstake_bonded_outcome_c() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        contract.dr_unstake(
            U64(0),
            0,
            data_request::Outcome::Answer(AnswerType::String("c".to_string())),
            U128(1),
        );
    }

    #[test]
    #[should_panic(
        expected = "alice.near has less staked on this outcome (10) than unstake amount"
    )]
    fn dr_unstake_too_much() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            10,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_unstake(
            U64(0),
            0,
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            U128(11),
        );
    }

    #[test]
    fn dr_unstake_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let outcome = data_request::Outcome::Answer(AnswerType::String("b".to_string()));
        contract.dr_stake(
            alice(),
            10,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        testing_env!(get_context(alice()));

        // verify initial storage
        assert_eq!(
            contract
                .dr_get_expect_active(0.into())
                .resolution_windows
                .get(0)
                .unwrap()
                .user_to_outcome_to_stake
                .get(&alice())
                .unwrap()
                .get(&outcome)
                .unwrap(),
            10
        );
        assert_eq!(
            contract
                .dr_get_expect_active(0.into())
                .resolution_windows
                .get(0)
                .unwrap()
                .outcome_to_stake
                .get(&outcome)
                .unwrap(),
            10
        );

        contract.dr_unstake(
            U64(0),
            0,
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            U128(1),
        );

        // verify storage after unstake
        assert_eq!(
            contract
                .dr_get_expect_active(0.into())
                .resolution_windows
                .get(0)
                .unwrap()
                .user_to_outcome_to_stake
                .get(&alice())
                .unwrap()
                .get(&outcome)
                .unwrap(),
            9
        );
        assert_eq!(
            contract
                .dr_get_expect_active(0.into())
                .resolution_windows
                .get(0)
                .unwrap()
                .outcome_to_stake
                .get(&outcome)
                .unwrap(),
            9
        );
    }

    #[test]
    #[should_panic(expected = "Error no DataRequest with this id exists")]
    fn dr_claim_invalid_id() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_claim(alice(), U64(0));
    }

    #[test]
    fn dr_claim_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        contract.dr_claim(alice(), U64(0));
    }

    #[test]
    fn d_claim_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 200);
    }

    #[test]
    fn d_claim_same_twice() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 200);
        assert_eq!(sum_claim_res(d.claim(alice())), 0);
    }

    #[test]
    fn d_validity_bond() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.min_resolution_bond = U128(2);
        config.validity_bond = U128(2);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());

        // fees (100% of TVL)
        assert_eq!(sum_claim_res(d.claim(alice())), 294);
    }

    #[test]
    fn d_claim_double() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 100);
        assert_eq!(sum_claim_res(d.claim(bob())), 100);
    }

    #[test]
    fn d_claim_2rounds_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond + round 0 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 600);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
    }

    #[test]
    fn d_claim_2rounds_double() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            carol(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond + round 0 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 450);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
        assert_eq!(sum_claim_res(d.claim(carol())), 150);
    }

    #[test]
    fn d_claim_3rounds_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            carol(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 1120);
        // validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 280);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
    }

    #[test]
    fn d_claim_3rounds_double_round0() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            dave(),
            100,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            carol(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 1120);
        // 50% of validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 140);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
        // 50% of validity bond
        assert_eq!(sum_claim_res(d.claim(dave())), 140);
    }

    #[test]
    fn d_claim_3rounds_double_round2() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(
            bob(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            carol(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        contract.dr_stake(
            dave(),
            300,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // 5/8 of round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 700);
        // validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 280);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
        // 3/8 of round 1 stake
        assert_eq!(sum_claim_res(d.claim(dave())), 420);
    }

    #[test]
    fn d_claim_final_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config_for_quick_final_arb());
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        assert_eq!(sum_claim_res(d.claim(alice())), 600);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
    }

    #[test]
    fn d_claim_final_arb_extra_round() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(600);
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(
            carol(),
            800,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 280);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
        // round 1 funds
        assert_eq!(sum_claim_res(d.claim(carol())), 1120);
    }

    #[test]
    fn d_claim_final_arb_extra_round2() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(600);
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(
            carol(),
            800,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        assert_eq!(sum_claim_res(d.claim(alice())), 0);
        // validity bond (100), round0 (200), round2 (800)
        assert_eq!(sum_claim_res(d.claim(bob())), 1400);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
    }

    #[test]
    #[should_panic(expected = "Final arbitrator is invoked for `DataRequest` with id: 0")]
    fn dr_final_arb_invoked() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config_for_quick_final_arb());
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );
        contract.dr_stake(
            carol(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
    }

    #[test]
    #[should_panic(expected = "Incompatible outcome")]
    fn dr_final_arb_invalid_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("c".to_string())),
        );
    }

    #[test]
    #[should_panic(
        expected = "assertion failed: `(left == right)`\n  left: `\"alice.near\"`,\n right: `\"bob.near\"`: sender is not the final arbitrator of this `DataRequest`, the final arbitrator is: alice.near"
    )]
    fn dr_final_arb_non_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );

        testing_env!(get_context(bob()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );
    }

    #[test]
    #[should_panic(expected = "Error DataRequest is already finalized")]
    fn dr_final_arb_twice() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config_for_quick_final_arb());
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("a".to_string())),
        );
    }

    #[test]
    fn dr_final_arb_execute() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config_for_quick_final_arb());
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(
            alice(),
            200,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string())),
            },
        );
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(
            bob(),
            400,
            StakeDataRequestArgs {
                id: U64(0),
                outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string())),
            },
        );

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(
            U64(0),
            data_request::Outcome::Answer(AnswerType::String("b".to_string())),
        );

        let request: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        assert_eq!(request.resolution_windows.len(), 2);
        assert_eq!(
            request.finalized_outcome,
            data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        );
    }

    #[test]
    fn dr_tvl_increases() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let outcome = data_request::Outcome::Answer(AnswerType::String("b".to_string()));
        contract.dr_stake(
            alice(),
            10,
            StakeDataRequestArgs {
                id: U64(0),
                outcome,
            },
        );
    }

    #[test]
    fn dr_fixed_fee() {
        testing_env!(get_context(token()));
        let bob_requester = Requester {
            contract_name: bob(),
            account_id: bob(),
            stake_multiplier: None,
            code_base_url: None,
        };
        let fixed_fee = 20;
        let whitelist = Some(vec![bob_requester, registry_entry(carol())]);
        let mut config = config();
        config.min_resolution_bond = U128(2);
        let validity_bond = 2;
        config.validity_bond = U128(validity_bond);
        let mut contract = Contract::new(whitelist, config);
        contract.dr_new(
            bob(),
            fixed_fee + validity_bond,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
        dr_finalize(
            &mut contract,
            data_request::Outcome::Answer(data_request::AnswerType::String("a".to_string())),
        );

        let mut d: FinalizedDataRequest = contract.dr_get_expect_finalized(0.into());
        assert_eq!(sum_claim_res(d.claim(alice())), 60);
    }

    #[test]
    fn dr_get_methods() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_new(&mut contract);
        dr_new(&mut contract);

        match contract.get_latest_request().unwrap() {
            DataRequestSummary::Active(s) => assert_eq!(s.id, U64(2)),
            DataRequestSummary::Finalized(_) => panic!("expected `DataRequest` to be active"),
        }
        match contract.get_request_by_id(U64(1)).unwrap() {
            DataRequestSummary::Active(s) => assert_eq!(s.id, U64(1)),
            DataRequestSummary::Finalized(_) => panic!("expected `DataRequest` to be active"),
        }
        match &contract.get_requests(U64(0), U64(1))[0] {
            DataRequestSummary::Active(s) => assert_eq!(s.id, U64(0)),
            DataRequestSummary::Finalized(_) => panic!("expected `DataRequest` to be active"),
        }
        assert_eq!(contract.get_requests(U64(1), U64(1)).len(), 1);
        assert_eq!(contract.get_requests(U64(1), U64(2)).len(), 2);
        assert_eq!(contract.get_requests(U64(0), U64(3)).len(), 3);
    }

    #[test]
    #[should_panic(expected = "Oracle is paused")]
    fn dr_pausable() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        testing_env!(get_context(gov()));
        contract.toggle_pause();

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec!["1".to_string()],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );
    }

    #[test]
    fn dr_new_first_party_oracle_fetch_test() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        let outcome = Outcome::Answer(AnswerType::String("1_000_000".to_string()));

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec![],
                data_type: data_request::DataRequestDataType::String,
                provider: Some(bob()),
            },
        );

        testing_env!(get_context(bob()));
        contract.dr_finalize_by_provider(U64(0), outcome);
    }

    #[test]
    #[should_panic(expected = "error this is not a provider data request")]
    fn dr_finalize_non_provider_request() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        let outcome = Outcome::Answer(AnswerType::String("1_000_000".to_string()));

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec![],
                data_type: data_request::DataRequestDataType::String,
                provider: None,
            },
        );

        testing_env!(get_context(carol()));
        contract.dr_finalize_by_provider(U64(0), outcome);
    }

    #[test]
    #[should_panic(expected = "this request can only be finalized by the provider")]
    fn dr_finalize_from_wrong_provider() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        let outcome = Outcome::Answer(AnswerType::String("1_000_000".to_string()));

        contract.dr_new(
            bob(),
            100,
            NewDataRequestArgs {
                sources: Some(Vec::new()),
                outcomes: None,
                challenge_period: U64(1500),
                description: Some("a".to_string()),
                tags: vec![],
                data_type: data_request::DataRequestDataType::String,
                provider: Some(bob()),
            },
        );

        testing_env!(get_context(carol()));
        contract.dr_finalize_by_provider(U64(0), outcome);
    }
}
