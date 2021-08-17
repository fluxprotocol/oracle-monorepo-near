use crate::*;

use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::json_types::{U64, U128};
use near_sdk::serde::{ Deserialize, Serialize };
use near_sdk::{ env, Balance, AccountId, PromiseOrValue, Promise, ext_contract };
use near_sdk::collections::{ Vector };

use crate::resolution_window::*;
use crate::types::*;
use crate::logger;
use crate::fungible_token::{ fungible_token_transfer };

pub const PERCENTAGE_DIVISOR: u16 = 10_000;

pub const FINALZATION_GAS: u64 = 250_000_000_000_000;

#[ext_contract]
trait ExtSelf {
    fn dr_proceed_finalization(request_id: U64, sender: AccountId);
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Clone)]
pub struct Source {
    pub end_point: String,
    pub source_path: String
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq)]
pub enum DataRequestDataType {
    Number(U128),
    String,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct DataRequest {
    pub id: u64,
    pub description: Option<String>,
    pub sources: Vec<Source>,
    pub outcomes: Option<Vec<String>>,
    pub requestor: AccountId, // Request Interface contract
    pub creator: AccountId, // The account that created the request (account to return validity bond to)
    pub finalized_outcome: Option<Outcome>,
    pub resolution_windows: Vector<ResolutionWindow>,
    pub global_config_id: u64, // Config id
    pub request_config: DataRequestConfig,
    pub settlement_time: u64,
    pub initial_challenge_period: Duration,
    pub final_arbitrator_triggered: bool,
    pub target_contract: target_contract_handler::TargetContract,
    pub tags: Option<Vec<String>>,
    pub data_type: DataRequestDataType,
    pub paid_fee: Option<u128>
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct DataRequestSummary {
    pub id: u64,
    pub description: Option<String>,
    pub sources: Vec<Source>,
    pub outcomes: Option<Vec<String>>,
    pub requestor: AccountId,
    pub creator: AccountId,
    pub finalized_outcome: Option<Outcome>,
    pub resolution_windows: Vec<ResolutionWindowSummary>,
    pub global_config_id: U64,
    pub settlement_time: U64,
    pub initial_challenge_period: U64,
    pub final_arbitrator_triggered: bool,
    pub target_contract: AccountId,
    pub tags: Option<Vec<String>>,
    pub paid_fee: Option<WrappedBalance>
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize)]
pub enum CustomFeeStake {
    Multiplier(u16),
    Fixed(Balance),
    None
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct DataRequestConfig {
    default_challenge_window_duration: Duration,
    final_arbitrator_invoke_amount: Balance,
    final_arbitrator: AccountId,
    validity_bond: Balance,
    pub fee: Balance,
    pub custom_fee: CustomFeeStake
}

trait DataRequestChange {
    fn new(sender: AccountId, id: u64, global_config_id: u64, global_config: &oracle_config::OracleConfig, tvl_of_requestor: Balance, custom_fee: CustomFeeStake, request_data: NewDataRequestArgs) -> Self;
    fn stake(&mut self, sender: AccountId, outcome: Outcome, amount: Balance) -> Balance;
    fn unstake(&mut self, sender: AccountId, round: u16, outcome: Outcome, amount: Balance) -> Balance;
    fn finalize(&mut self);
    fn invoke_final_arbitrator(&mut self, bond_size: Balance) -> bool;
    fn finalize_final_arbitrator(&mut self, outcome: Outcome);
    fn claim(&mut self, account_id: String) -> ClaimRes;
    fn return_validity_bond(&self, token: AccountId) -> PromiseOrValue<bool>;
}

impl DataRequestChange for DataRequest {
    fn new(
        sender: AccountId,
        id: u64,
        global_config_id: u64,
        config: &oracle_config::OracleConfig,
        tvl_of_requestor: Balance,
        custom_fee: CustomFeeStake,
        request_data: NewDataRequestArgs
    ) -> Self {
        let resolution_windows = Vector::new(format!("rw{}", id).as_bytes().to_vec());

        // TODO: this has to be reworked
        // set fee to fixed fee if set, otherwise set to percentage of requestor's TVL
        let fee: Balance = match custom_fee {
            CustomFeeStake::Fixed(f) => f.into(),
            CustomFeeStake::Multiplier(_) | CustomFeeStake::None =>
                config.fee.resolution_fee_percentage as Balance * 
                tvl_of_requestor / 
                PERCENTAGE_DIVISOR as Balance
        };

        Self {
            id,
            sources: request_data.sources,
            outcomes: request_data.outcomes,
            requestor: sender,
            finalized_outcome: None,
            resolution_windows,
            global_config_id,
            request_config: DataRequestConfig {
                default_challenge_window_duration: config.default_challenge_window_duration.into(),
                final_arbitrator_invoke_amount: config.final_arbitrator_invoke_amount.into(),
                final_arbitrator: config.final_arbitrator.to_string(),
                validity_bond: config.validity_bond.into(),
                custom_fee,
                fee
            },
            initial_challenge_period: request_data.challenge_period.into(),
            settlement_time: request_data.settlement_time.into(),
            final_arbitrator_triggered: false,
            target_contract: target_contract_handler::TargetContract(request_data.target_contract),
            description: request_data.description,
            tags: request_data.tags,
            data_type: request_data.data_type,
            creator: request_data.creator,
            paid_fee: None
        }
    }

    // @returns amount of tokens that didn't get staked
    fn stake(&mut self,
        sender: AccountId,
        outcome: Outcome,
        amount: Balance
    ) -> Balance {
        let mut window = self.resolution_windows
            .iter()
            .last()
            .unwrap_or_else( || {
                ResolutionWindow::new(self.id, 0, self.calc_resolution_bond(), self.initial_challenge_period, env::block_timestamp())
            });

        let unspent = window.stake(sender, outcome, amount);

        // If first window push it to vec, else replace updated window struct
        if self.resolution_windows.len() == 0 {
            self.resolution_windows.push(&window);
        } else {
            self.resolution_windows.replace(
                self.resolution_windows.len() - 1, // Last window
                &window
            );
        }
        // Check if this stake is bonded for the current window and if the final arbitrator should be invoked.
        // If the final arbitrator is invoked other stake won't come through.
        if window.bonded_outcome.is_some() && !self.invoke_final_arbitrator(window.bond_size) {
            self.resolution_windows.push(
                &ResolutionWindow::new(
                    self.id,
                    self.resolution_windows.len() as u16,
                    window.bond_size,
                    self.request_config.default_challenge_window_duration,
                    env::block_timestamp()
                )
            );
        }

        unspent
    }

    // @returns amount of tokens that didn't get staked
    fn unstake(&mut self, sender: AccountId, round: u16, outcome: Outcome, amount: Balance) -> Balance {        
        let mut window = self.resolution_windows
            .get(round as u64)
            .expect("ERR_NO_RESOLUTION_WINDOW");

        window.unstake(sender, outcome, amount)
    }

    fn finalize(&mut self) {
        self.finalized_outcome = self.get_final_outcome();
    }

    // @returns wether final arbitrator was triggered
    fn invoke_final_arbitrator(&mut self, bond_size: Balance) -> bool {
        let should_invoke = bond_size >= self.request_config.final_arbitrator_invoke_amount;
        if should_invoke { self.final_arbitrator_triggered = true }
        self.final_arbitrator_triggered
    }

    fn finalize_final_arbitrator(&mut self, outcome: Outcome) {
        self.finalized_outcome = Some(outcome);
    }

    fn claim(&mut self, account_id: String) -> ClaimRes {
        // Metrics for calculating payout
        let mut total_correct_staked = 0;
        let mut total_incorrect_staked = 0;
        let mut user_correct_stake = 0;

        // For any round after the resolution round handle generically
        for round in 0..self.resolution_windows.len() {
            let mut window = self.resolution_windows.get(round).unwrap();
            let stake_state: WindowStakeResult = window.claim_for(account_id.to_string(), self.finalized_outcome.as_ref().unwrap());
            match stake_state {
                WindowStakeResult::Correct(correctly_staked) => {
                    total_correct_staked += correctly_staked.bonded_stake;
                    user_correct_stake += correctly_staked.user_stake;
                },
                WindowStakeResult::Incorrect(incorrectly_staked) => {
                    total_incorrect_staked += incorrectly_staked
                },
                WindowStakeResult::NoResult => ()
            }

            self.resolution_windows.replace(round as u64, &window);
        };

        let stake_profit = match total_correct_staked {
            0 => 0,
            _ => helpers::calc_product(user_correct_stake, total_incorrect_staked, total_correct_staked)
        };

        let payment_token_payout = self.calc_resolution_fee_payout();

        let fee_bond_profit = match total_correct_staked {
            0 => 0,
            _ => helpers::calc_product(user_correct_stake, payment_token_payout, total_correct_staked)
        };

        logger::log_claim(&account_id, self.id, total_correct_staked, total_incorrect_staked, user_correct_stake, stake_profit);

        ClaimRes {
            payment_token_payout: fee_bond_profit,
            stake_token_payout: user_correct_stake + stake_profit
        }
    }

    // @notice Return what's left of validity_bond to requestor
    fn return_validity_bond(&self, token: AccountId) -> PromiseOrValue<bool> {
        let bond_to_return = self.calc_validity_bond_to_return();

        if bond_to_return > 0 {
            return PromiseOrValue::Promise(fungible_token_transfer(token, self.creator.clone(), bond_to_return))
        }

        PromiseOrValue::Value(false)
    }
}

trait DataRequestView {
    fn assert_valid_outcome(&self, outcome: &Outcome);
    fn assert_valid_outcome_type(&self, outcome: &Outcome);
    fn assert_can_stake_on_outcome(&self, outcome: &Outcome);
    fn assert_not_finalized(&self);
    fn assert_finalized(&self);
    fn assert_can_finalize(&self, gov: AccountId);
    fn assert_can_claim_fee(&self);
    fn assert_final_arbitrator(&self);
    fn assert_final_arbitrator_invoked(&self);
    fn assert_final_arbitrator_not_invoked(&self);
    fn assert_reached_settlement_time(&self);
    fn get_final_outcome(&self) -> Option<Outcome>;
    fn calc_resolution_bond(&self) -> Balance;
    fn calc_validity_bond_to_return(&self) -> Balance;
    fn calc_resolution_fee_payout(&self) -> Balance;
    fn summarize_dr(&self) -> DataRequestSummary;
}

impl DataRequestView for DataRequest {
    fn assert_valid_outcome(&self, outcome: &Outcome) {
        match &self.outcomes {
            Some(outcomes) => match outcome {
                Outcome::Answer(outcome) => {
                    // Only strings can be staked when an array of outcomes are set
                    match outcome {
                        AnswerType::String(string_answer) => assert!(outcomes.contains(&string_answer), "Incompatible outcome"),
                        _ => panic!("ERR_OUTCOME_NOT_STRING"),
                    };


                }
                Outcome::Invalid => ()
            },
            None => ()
        }
    }

    fn assert_valid_outcome_type(&self, outcome: &Outcome) {
        match outcome {
            Outcome::Answer(answer) => {
                match answer {
                    AnswerType::String(_) => assert_eq!(self.data_type, DataRequestDataType::String, "ERR_WRONG_OUTCOME_TYPE"),
                    AnswerType::Number(ans_num) => {
                        match self.data_type {
                            DataRequestDataType::Number(dr_multiplier) => assert_eq!(dr_multiplier, ans_num.multiplier, "ERR_WRONG_MULTIPLIER"),
                            _ => panic!("ERR_WRONG_OUTCOME_TYPE"),
                        }
                    }
                }
            }
            _ => ()
        }
    }

    fn assert_can_stake_on_outcome(&self, outcome: &Outcome) {
        if self.resolution_windows.len() > 1 {
            let last_window = self.resolution_windows.get(self.resolution_windows.len() - 2).unwrap();
            // TODO, currently checking references are equal. In my experience checking values is safer.
            assert_ne!(&last_window.bonded_outcome.unwrap(), outcome, "Outcome is incompatible for this round");
        }
    }

    fn assert_not_finalized(&self) {
        assert!(self.finalized_outcome.is_none(), "Can't stake in finalized DataRequest");
    }

    fn assert_finalized(&self) {
        assert!(self.finalized_outcome.is_some(), "DataRequest is not finalized");
    }

    fn assert_can_claim_fee(&self){
        assert!(self.resolution_windows.iter().count() >= 2, "No bonded outcome found");
        let last_window = self.resolution_windows.iter().last().unwrap();
        assert!(self.paid_fee.is_none());
        assert!(env::block_timestamp() >= last_window.end_time, "Challenge period not ended");
    }

    fn assert_can_finalize(&self, gov: AccountId) {
        assert!(!self.final_arbitrator_triggered, "Can only be finalized by final arbitrator: {}", self.request_config.final_arbitrator);
        assert!(self.paid_fee.is_some() || env::predecessor_account_id() == gov, "Can only be finalized after fee is paid or when enforced by the DAO");
        self.assert_not_finalized();
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

    fn assert_reached_settlement_time(&self) {
        assert!(
            self.settlement_time <= env::block_timestamp(),
            "Cannot stake on `DataRequest` {} until settlement time {}",
            self.id,
            self.settlement_time
        );
    }

    fn get_final_outcome(&self) -> Option<Outcome> {
        assert!(self.resolution_windows.iter().count() >= 2, "No bonded outcome found");
        let last_bonded_window_i = self.resolution_windows.len() - 2; // Last window after end_time never has a bonded outcome
        let last_bonded_window = self.resolution_windows.get(last_bonded_window_i).unwrap();
        last_bonded_window.bonded_outcome
    }

    /**
     * @notice Calculates the size of the resolution bond. If the accumulated fee is smaller than the validity bond, we payout the validity bond to validators, thus they have to stake double in order to be
     * eligible for the reward, in the case that the fee is greater than the validity bond validators need to have a cumulative stake of double the fee amount
     * @returns The size of the initial `resolution_bond` denominated in `stake_token`
     */
    fn calc_resolution_bond(&self) -> Balance {
        let resolution_bond = match self.request_config.custom_fee {
            CustomFeeStake::None => {
                if self.request_config.fee > self.request_config.validity_bond {
                    self.request_config.fee
                } else {
                    self.request_config.validity_bond
                }
            },
            CustomFeeStake::Multiplier(m) => {
                let weighted_validity_bond = helpers::calc_product(
                    self.request_config.validity_bond,
                    u128::from(m),
                    PERCENTAGE_DIVISOR as Balance
                );
                if self.request_config.fee > weighted_validity_bond {
                    self.request_config.fee
                } else {
                    weighted_validity_bond
                }
            },
            CustomFeeStake::Fixed(_) => self.request_config.validity_bond
        };
        resolution_bond
    }

     /**
     * @notice Calculates, how much of, the `validity_bond` should be returned to the creator, if the fees accrued are less than the validity bond, only return the fees accrued to the creator.
     * The rest of the bond will be paid out to resolvers. If the `DataRequest` is invalid the fees and the `validity_bond` are paid out to resolvers, the creator gets slashed.
     * @returns How much of the `validity_bond` should be returned to the creator after resolution denominated in `stake_token`
     */
    fn calc_validity_bond_to_return(&self) -> Balance {
        let outcome = self.finalized_outcome.as_ref().unwrap();
        match outcome {
            Outcome::Answer(_) => {
                if self.request_config.fee > self.request_config.validity_bond {
                    self.request_config.validity_bond
                } else {
                    self.request_config.fee
                }
            },
            Outcome::Invalid => 0
        }
    }

    /**
     * @notice Calculates the size of the resolution bond. If the accumulated fee is smaller than the validity bond, we payout the validity bond to validators, thus they have to stake double in order to be
     * eligible for the reward, in the case that the fee is greater than the validity bond validators need to have a cumulative stake of double the fee amount
     * @returns The size of the resolution fee paid out to resolvers denominated in `stake_token`
     */
    fn calc_resolution_fee_payout(&self) -> Balance {
        if self.paid_fee.unwrap_or(0) > self.request_config.validity_bond {
            self.request_config.fee
        } else {
            self.request_config.validity_bond
        }
    }

    /**
     * @notice Transforms a data request struct into another struct with Serde serialization
     */
    fn summarize_dr(&self) -> DataRequestSummary {
        // format resolution windows inside this data request
        let mut resolution_windows = Vec::new();
        for i in self.resolution_windows.iter() {
            let rw = ResolutionWindowSummary {
                round: i.round,
                start_time: i.start_time,
                end_time: i.end_time,
                bond_size: i.bond_size,
                bonded_outcome: i.bonded_outcome,
            };
            resolution_windows.push(rw);
        }

        let paid_fee = match self.paid_fee {
            Some(paid_fee) => Some(U128(paid_fee)),
            None => None
        };
        // format data request
        DataRequestSummary {
            id: self.id,
            description: self.description.clone(),
            sources: self.sources.clone(),
            outcomes: self.outcomes.clone(),
            requestor: self.requestor.clone(),
            creator: self.creator.clone(),
            finalized_outcome: self.finalized_outcome.clone(),
            resolution_windows: resolution_windows,
            global_config_id: U64(self.global_config_id),
            settlement_time: U64(self.settlement_time),
            initial_challenge_period: U64(self.initial_challenge_period),
            final_arbitrator_triggered: self.final_arbitrator_triggered,
            target_contract: self.target_contract.0.clone(),
            tags: self.tags.clone(),
            paid_fee: paid_fee
        }
    }
}

#[near_bindgen]
impl Contract {
    pub fn dr_exists(&self, id: U64) -> bool {
        self.data_requests.get(id.into()).is_some()
    }

    // Merge config and payload
    pub fn dr_new(&mut self, sender: AccountId, amount: Balance, tvl_of_requestor: Balance, payload: NewDataRequestArgs) -> Balance {
        let config = self.get_config();
        let validity_bond: u128 = config.validity_bond.into();
        self.assert_whitelisted(sender.to_string());
        self.assert_sender(&config.payment_token);
        self.dr_validate(&payload);
        assert!(amount >=validity_bond, "Validity bond not reached");

        let requestor_custom_fee: CustomFeeStake = self.whitelist_custom_fee(sender.to_string());

        let dr = DataRequest::new(
            sender,
            self.data_requests.len() as u64,
            self.configs.len() - 1, // TODO: should probably trim down once we know what attributes we need stored for `DataRequest`s
            &config,
            tvl_of_requestor,
            requestor_custom_fee,
            payload
        );

        logger::log_new_data_request(&dr);

        self.data_requests.push(&dr);

        // calc amount to return to sender
        let amount_to_send = amount - validity_bond;

        if amount > validity_bond {
            amount_to_send
        } else {
            0
        }
    }

    #[payable]
    pub fn dr_stake(&mut self, sender: AccountId, amount: Balance, payload: StakeDataRequestArgs) -> PromiseOrValue<WrappedBalance> {
        let mut dr = self.dr_get_expect(payload.id.into());
        let config = self.configs.get(dr.global_config_id).unwrap();
        self.assert_sender(&config.stake_token);
        dr.assert_reached_settlement_time();
        dr.assert_final_arbitrator_not_invoked();
        dr.assert_can_stake_on_outcome(&payload.outcome);
        dr.assert_valid_outcome(&payload.outcome);
        dr.assert_valid_outcome_type(&payload.outcome);
        dr.assert_not_finalized();

        let unspent_stake = dr.stake(sender, payload.outcome, amount);
        let _spent_stake = amount - unspent_stake;

        logger::log_update_data_request(&dr);
        self.data_requests.replace(payload.id.into(), &dr);

        PromiseOrValue::Value(U128(unspent_stake))
    }

    #[payable]
    pub fn dr_unstake(&mut self, request_id: U64, resolution_round: u16, outcome: Outcome, amount: U128) {
        let initial_storage = env::storage_usage();

        let mut dr = self.dr_get_expect(request_id.into());
        let unstaked = dr.unstake(env::predecessor_account_id(), resolution_round, outcome, amount.into());
        let config = self.configs.get(dr.global_config_id).unwrap();

        helpers::refund_storage(initial_storage, env::predecessor_account_id());
        logger::log_update_data_request(&dr);

        fungible_token_transfer(config.stake_token, env::predecessor_account_id(), unstaked);
    }

    /**
     * @returns amount of tokens claimed
     */
    #[payable]
    pub fn dr_claim(&mut self, account_id: String, request_id: U64) -> Promise {
        let initial_storage = env::storage_usage();

        let mut dr = self.dr_get_expect(request_id.into());
        dr.assert_finalized();
        let stake_payout = dr.claim(account_id.to_string());
        let config = self.configs.get(dr.global_config_id).unwrap();

        logger::log_update_data_request(&dr);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());

        // transfer owed stake tokens
        let prev_prom = if stake_payout.stake_token_payout > 0 {
            Some(fungible_token_transfer(config.stake_token, account_id.to_string(), stake_payout.stake_token_payout))
        } else {
            None
        };
        
        if stake_payout.payment_token_payout > 0 {
            // distribute fee + bond
            match prev_prom {
                Some(p) => p.then(fungible_token_transfer(config.payment_token, account_id, stake_payout.payment_token_payout)),
                None => fungible_token_transfer(config.payment_token, account_id, stake_payout.payment_token_payout)
            }
        } else {
            match prev_prom {
                Some(p) => p,
                None => panic!("can't claim 0")
            }
        }
    }

    #[payable]
    pub fn dr_claim_fee(&mut self, request_id: U64) -> Promise {
        helpers::assert_one_yocto();
        let dr = self.dr_get_expect(request_id.into());
        let config = self.get_config();
        dr.assert_can_claim_fee();

        dr.target_contract.claim_fee(request_id, config.fee.resolution_fee_percentage)
    }

    pub fn set_claimed_fee(&mut self, amount: u128, sender: AccountId, dr_id: u64) -> PromiseOrValue<U128> {
        let mut dr = self.dr_get_expect(dr_id.into());
        let config = self.configs.get(dr.global_config_id).unwrap();
        self.assert_sender(&config.payment_token);
        assert_eq!(sender, dr.target_contract.0, "Can only be called by the target contract");
        dr.assert_can_claim_fee();

        dr.paid_fee = Some(amount);
        self.data_requests.replace(dr_id, &dr);
        
        PromiseOrValue::Value(U128(0))
    }

    pub fn dr_finalize(&mut self, request_id: U64) -> Promise {
        let dr = self.dr_get_expect(request_id.into());
        let config = self.configs.get(dr.global_config_id).unwrap();
        dr.assert_can_finalize(config.gov);
        let final_outcome = dr.get_final_outcome();
        dr.target_contract.set_outcome(request_id, dr.requestor.clone(), final_outcome.unwrap(), dr.tags.clone(), false)
        .then(
            ext_self::dr_proceed_finalization(
                request_id,
                env::predecessor_account_id(),
                // NEAR Params
                &env::current_account_id(),
                0,
                FINALZATION_GAS
            )
        )
    }

    #[private]
    pub fn dr_proceed_finalization(&mut self, request_id: U64, sender: AccountId) {        
        let mut dr = self.dr_get_expect(request_id.into());
        let config = self.configs.get(dr.global_config_id).unwrap();
        assert!(helpers::previous_promise_successful() || sender == config.gov, "finalization transaction failed, can only be bypassed by `gov`");
        dr.assert_can_finalize(config.gov); // prevent race conditions

        dr.finalize();

        dr.return_validity_bond(config.payment_token);

        self.data_requests.replace(request_id.into(), &dr);

        logger::log_update_data_request(&dr);
    }

    #[payable]
    pub fn dr_final_arbitrator_finalize(&mut self, request_id: U64, outcome: Outcome) -> PromiseOrValue<bool> {
        let initial_storage = env::storage_usage();

        let mut dr = self.dr_get_expect(request_id);
        dr.assert_not_finalized();
        dr.assert_final_arbitrator();
        dr.assert_valid_outcome(&outcome);
        dr.assert_final_arbitrator_invoked();
        dr.finalize_final_arbitrator(outcome.clone());

        let config = self.configs.get(dr.global_config_id).unwrap();
        dr.target_contract.set_outcome(request_id, dr.requestor.clone(), outcome, dr.tags.clone(), true);
        self.data_requests.replace(request_id.into(), &dr);

        logger::log_update_data_request(&dr);
        helpers::refund_storage(initial_storage, env::predecessor_account_id());

        dr.return_validity_bond(config.payment_token)
    }

    fn dr_get_expect(&self, id: U64) -> DataRequest {
        self.data_requests.get(id.into()).expect("ERR_DATA_REQUEST_NOT_FOUND")
    }

    pub fn get_request_by_id(&self, id: U64) -> Option<DataRequestSummary> {
        let dr = self.data_requests.get(id.into());
        match dr {
            None => None,
            Some(d) => Some(d.summarize_dr())
        }
    }

    pub fn get_latest_request(&self) -> Option<DataRequestSummary> {
        if self.data_requests.len() < 1 {
            return None;
        }
        let dr = self.data_requests.get(self.data_requests.len() - 1);
        match dr {
            None => None,
            Some(d) => Some(d.summarize_dr())
        }
    }

    pub fn get_requests(&self, from_index: U64, limit: U64) -> Vec<DataRequestSummary> {
        let i: u64 = from_index.into();
        (i..std::cmp::min(i + u64::from(limit), self.data_requests.len()))
            .map(|index| self.data_requests.get(index).unwrap().summarize_dr())
            .collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod mock_token_basic_tests {
    use near_sdk::{ MockedBlockchain };
    use near_sdk::{ testing_env, VMContext };
    use crate::whitelist::{CustomFeeStakeArgs, RegistryEntry};
    use crate::data_request::AnswerType;
    use super::*;
    use fee_config::FeeConfig;

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

    fn target() -> AccountId {
        "target.near".to_string()
    }

    fn gov() -> AccountId {
        "gov.near".to_string()
    }

    fn sum_claim_res(claim_res: ClaimRes) -> u128 {
        claim_res.payment_token_payout + claim_res.stake_token_payout
    }

    fn registry_entry(account: AccountId) -> RegistryEntry {
        RegistryEntry {
            interface_name: account.clone(),
            contract_entry: account.clone(),
            custom_fee: CustomFeeStakeArgs::None,
            code_base_url: None
        }
    }

    fn finalize(contract: &mut Contract, dr_id: u64) -> &mut Contract {
        let mut dr = contract.dr_get_expect(U64(dr_id));
        dr.finalize();
        contract.data_requests.replace(0, &dr);
        contract
    }

    fn config() -> oracle_config::OracleConfig {
        oracle_config::OracleConfig {
            gov: gov(),
            final_arbitrator: alice(),
            payment_token: token(),
            stake_token: token(),
            validity_bond: U128(100),
            max_outcomes: 8,
            default_challenge_window_duration: U64(1000),
            min_initial_challenge_window_duration: U64(1000),
            final_arbitrator_invoke_amount: U128(250),
            fee: FeeConfig {
                flux_market_cap: U128(50000),
                total_value_staked: U128(10000),
                resolution_fee_percentage: 10_000,
            }
        }
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

        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }


    #[test]
    #[should_panic(expected = "Err predecessor is not whitelisted")]
    fn dr_new_non_whitelisted() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(alice(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(0),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "This function can only be called by token.near")]
    fn dr_new_non_payment_token() {
        testing_env!(get_context(alice()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(0),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Too many sources provided, max sources is: 8")]
    fn dr_new_arg_source_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        let x1 = data_request::Source {end_point: "1".to_string(), source_path: "1".to_string()};
        let x2 = data_request::Source {end_point: "2".to_string(), source_path: "2".to_string()};
        let x3 = data_request::Source {end_point: "3".to_string(), source_path: "3".to_string()};
        let x4 = data_request::Source {end_point: "4".to_string(), source_path: "4".to_string()};
        let x5 = data_request::Source {end_point: "5".to_string(), source_path: "5".to_string()};
        let x6 = data_request::Source {end_point: "6".to_string(), source_path: "6".to_string()};
        let x7 = data_request::Source {end_point: "7".to_string(), source_path: "7".to_string()};
        let x8 = data_request::Source {end_point: "8".to_string(), source_path: "8".to_string()};
        let x9 = data_request::Source {end_point: "9".to_string(), source_path: "9".to_string()};
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: vec![x1,x2,x3,x4,x5,x6,x7,x8,x9],
            outcomes: None,
            challenge_period: U64(1000),
            settlement_time: U64(0),
            target_contract: target(),
            description: None,
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Invalid outcome list either exceeds min of: 2 or max of 8")]
    fn dr_new_arg_outcome_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
                "6".to_string(),
                "7".to_string(),
                "8".to_string(),
                "9".to_string()
            ]),
            challenge_period: U64(1000),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Description should be filled when no sources are given")]
    fn dr_description_required_no_sources() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: vec![],
            outcomes: None,
            challenge_period: U64(1000),
            settlement_time: U64(0),
            target_contract: target(),
            description: None,
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Challenge shorter than minimum challenge period")]
    fn dr_new_arg_challenge_period_below_min() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(999),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Challenge period exceeds maximum challenge period")]
    fn dr_new_arg_challenge_period_exceed() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(3001),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "Validity bond not reached")]
    fn dr_new_not_enough_amount() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 90, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    fn dr_new_success_exceed_amount() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        let amount : Balance = contract.dr_new(bob(), 200, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
        assert_eq!(amount, 100);
    }

    #[test]
    fn dr_new_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        let amount : Balance = contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: None,
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
        assert_eq!(amount, 0);
    }

    fn dr_new(contract : &mut Contract) {
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
    }

    #[test]
    #[should_panic(expected = "This function can only be called by token.near")]
    fn dr_stake_non_stake_token() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        testing_env!(get_context(alice()));
        contract.dr_stake(alice(),100,  StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string()))
        });
    }

    #[test]
    #[should_panic(expected = "ERR_DATA_REQUEST_NOT_FOUND")]
    fn dr_stake_not_existing() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_stake(alice(),100,  StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string()))
        });
    }

    #[test]
    #[should_panic(expected = "Incompatible outcome")]
    fn dr_stake_incompatible_answer() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(),100,  StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("42".to_string()))
        });
    }

    #[test]
    #[should_panic(expected = "Can't stake in finalized DataRequest")]
    fn dr_stake_finalized_market() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        let mut ct : VMContext = get_context(token());
        ct.block_timestamp = 1501;
        testing_env!(ct);

        let contract = finalize(&mut contract, 0);
        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
    }


    #[test]
    #[should_panic(expected = "Invalid outcome list either exceeds min of: 2 or max of 8")]
    fn dr_stake_finalized_settlement_time() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());

        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
    }

    #[test]
    fn dr_stake_success_partial() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let _b = contract.dr_stake(alice(), 5, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // assert_eq!(b, 0, "Invalid balance");

        let request : DataRequest = contract.data_requests.get(0).unwrap();
        assert_eq!(request.resolution_windows.len(), 1);


        let round0 : ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 1500);
        assert_eq!(round0.bond_size, 200);
    }

    #[test]
    fn dr_stake_success_full_at_t0() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let _b = contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // assert_eq!(b, 0, "Invalid balance");

        let request : DataRequest = contract.data_requests.get(0).unwrap();
        assert_eq!(request.resolution_windows.len(), 2);

        let round0 : ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 1500);
        assert_eq!(round0.bond_size, 200);

        let round1 : ResolutionWindow = request.resolution_windows.get(1).unwrap();
        assert_eq!(round1.round, 1);
        assert_eq!(round1.end_time, 1000);
        assert_eq!(round1.bond_size, 400);
    }

    #[test]
    fn dr_stake_success_overstake_at_t600() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let mut ct : VMContext = get_context(token());
        ct.block_timestamp = 600;
        testing_env!(ct);

        let _b = contract.dr_stake(alice(), 300, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // assert_eq!(b, 100, "Invalid balance");

        let request : DataRequest = contract.data_requests.get(0).unwrap();
        assert_eq!(request.resolution_windows.len(), 2);

        let round0 : ResolutionWindow = request.resolution_windows.get(0).unwrap();
        assert_eq!(round0.round, 0);
        assert_eq!(round0.end_time, 2100);
        assert_eq!(round0.bond_size, 200);

        let round1 : ResolutionWindow = request.resolution_windows.get(1).unwrap();
        assert_eq!(round1.round, 1);
        assert_eq!(round1.end_time, 1600);
        assert_eq!(round1.bond_size, 400);
    }

    #[test]
    #[should_panic(expected = "Can only be finalized by final arbitrator")]
    fn dr_finalize_final_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut c: oracle_config::OracleConfig = config();
        c.final_arbitrator_invoke_amount = U128(150);
        let mut contract = Contract::new(whitelist, c);
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        contract.dr_finalize(U64(0));
    }

    #[test]
    #[should_panic(expected = "No bonded outcome found")]
    fn dr_finalize_no_resolutions() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        finalize(&mut contract, 0);
    }

    #[test]
    #[should_panic(expected = "Can only be finalized after fee is paid or when enforced by the DAO")]
    fn dr_finalize_active_challenge() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        contract.dr_finalize(U64(0));
    }

    #[test]
    fn dr_finalize_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        let mut ct : VMContext = get_context(token());
        ct.block_timestamp = 1501;
        testing_env!(ct);

        let contract = finalize(&mut contract, 0);

        let request : DataRequest = contract.data_requests.get(0).unwrap();
        assert_eq!(request.resolution_windows.len(), 2);
        assert_eq!(request.finalized_outcome.unwrap(), data_request::Outcome::Answer(AnswerType::String("a".to_string())));
    }

    #[test]
    #[should_panic(expected = "Outcome is incompatible for this round")]
    fn dr_stake_same_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(), 300, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        contract.dr_stake(alice(), 500, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
    }


    fn dr_finalize(contract : &mut Contract, outcome : Outcome) {
        contract.dr_stake(alice(), 2000, StakeDataRequestArgs{
            id: U64(0),
            outcome: outcome
        });

        let mut ct : VMContext = get_context(token());
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

        contract.dr_unstake(U64(0), 0, data_request::Outcome::Answer(AnswerType::String("a".to_string())), U128(0));
    }

    #[test]
    #[should_panic(expected = "Cannot withdraw from bonded outcome")]
    fn dr_unstake_bonded_outcome() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        contract.dr_unstake(U64(0), 0, data_request::Outcome::Answer(AnswerType::String("a".to_string())), U128(0));
    }

    #[test]
    #[should_panic(expected = "token.near has less staked on this outcome (0) than unstake amount")]
    fn dr_unstake_bonded_outcome_c() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        contract.dr_unstake(U64(0), 0, data_request::Outcome::Answer(AnswerType::String("c".to_string())), U128(1));
    }

    #[test]
    #[should_panic(expected = "alice.near has less staked on this outcome (10) than unstake amount")]
    fn dr_unstake_too_much() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(alice(), 10, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_unstake(U64(0), 0, data_request::Outcome::Answer(AnswerType::String("b".to_string())), U128(11));
    }

    #[test]
    fn dr_unstake_success() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let outcome = data_request::Outcome::Answer(AnswerType::String("b".to_string()));
        contract.dr_stake(alice(), 10, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

        testing_env!(get_context(alice()));
        // TODO stake_token.balances should change?
        // verify initial storage
        assert_eq!(contract.
            data_requests.get(0).unwrap().
            resolution_windows.get(0).unwrap().
            user_to_outcome_to_stake.get(&alice()).unwrap().get(&outcome).unwrap(), 10);
        assert_eq!(contract.
            data_requests.get(0).unwrap().
            resolution_windows.get(0).unwrap().
            outcome_to_stake.get(&outcome).unwrap(), 10);

        contract.dr_unstake(U64(0), 0, data_request::Outcome::Answer(AnswerType::String("b".to_string())), U128(1));

        // verify storage after unstake
        assert_eq!(contract.
            data_requests.get(0).unwrap().
            resolution_windows.get(0).unwrap().
            user_to_outcome_to_stake.get(&alice()).unwrap().get(&outcome).unwrap(), 9);
        assert_eq!(contract.
            data_requests.get(0).unwrap().
            resolution_windows.get(0).unwrap().
            outcome_to_stake.get(&outcome).unwrap(), 9);
    }

    #[test]
    #[should_panic(expected = "ERR_DATA_REQUEST_NOT_FOUND")]
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
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        contract.dr_claim(alice(), U64(0));
    }

    #[test]
    fn d_claim_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 300);
    }

    #[test]
    fn d_claim_same_twice() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 300);
        assert_eq!(sum_claim_res(d.claim(alice())), 0);
    }

    #[test]
    fn d_validity_bond() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.validity_bond = U128(2);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // fees (100% of TVL)
        assert_eq!(sum_claim_res(d.claim(alice())), 15);
    }

    #[test]
    fn d_claim_double() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        contract.dr_stake(bob(), 100, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 150);
        assert_eq!(sum_claim_res(d.claim(bob())), 150);
    }

    #[test]
    fn d_claim_2rounds_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(bob(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("b".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond + round 0 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 700);
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

        contract.dr_stake(bob(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(carol(), 100, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("b".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond + round 0 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 525);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
        assert_eq!(sum_claim_res(d.claim(carol())), 175);
    }

    #[test]
    fn d_claim_3rounds_single() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(bob(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(carol(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 1200);
        // validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 300);
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

        contract.dr_stake(bob(), 100, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(dave(), 100, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(carol(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 1200);
        // 50% of validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 150);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
        // 50% of validity bond
        assert_eq!(sum_claim_res(d.claim(dave())), 150);
    }

    #[test]
    fn d_claim_3rounds_double_round2() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut config = config();
        config.final_arbitrator_invoke_amount = U128(1000);
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(bob(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(carol(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        contract.dr_stake(dave(), 300, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // 5/8 of round 1 stake
        assert_eq!(sum_claim_res(d.claim(alice())), 750);
        // validity bond
        assert_eq!(sum_claim_res(d.claim(bob())), 300);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
        // 3/8 of round 1 stake
        assert_eq!(sum_claim_res(d.claim(dave())), 450);
    }

    #[test]
    fn d_claim_final_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // TODO should be 500, validity bond (100) + last round (400)
        // assert_eq!(sum_claim_res(d.claim(alice())), 100);
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

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(carol(), 800, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("a".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        // validity bond
        assert_eq!(sum_claim_res(d.claim(alice())), 300);
        assert_eq!(sum_claim_res(d.claim(bob())), 0);
        // round 1 funds
        assert_eq!(sum_claim_res(d.claim(carol())), 1200);
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

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(carol(), 800, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("b".to_string())));

        let mut d = contract.data_requests.get(0).unwrap();
        assert_eq!(sum_claim_res(d.claim(alice())), 0);
        // validity bond (100), round0 (200), round2 (800)
        assert_eq!(sum_claim_res(d.claim(bob())), 1500);
        assert_eq!(sum_claim_res(d.claim(carol())), 0);
    }

    #[test]
    #[should_panic(expected = "Final arbitrator is invoked for `DataRequest` with id: 0")]
    fn dr_final_arb_invoked() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });
        contract.dr_stake(carol(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
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


        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("c".to_string())));
    }

    #[test]
    #[should_panic(expected = "assertion failed: `(left == right)`\n  left: `\"alice.near\"`,\n right: `\"bob.near\"`: sender is not the final arbitrator of this `DataRequest`, the final arbitrator is: alice.near")]
    fn dr_final_arb_non_arb() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);


        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });

        testing_env!(get_context(bob()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("b".to_string())));
    }

    #[test]
    #[should_panic(expected = "Can't stake in finalized DataRequest")]
    fn dr_final_arb_twice() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);


        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("b".to_string())));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("a".to_string())));
    }

    #[test]
    fn dr_final_arb_execute() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let config = config();
        let mut contract = Contract::new(whitelist, config);
        // needed for final arb function
        dr_new(&mut contract);

        contract.dr_stake(alice(), 200, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("a".to_string()))
        });
        // This round exceeds final arb limit, will be used as signal
        contract.dr_stake(bob(), 400, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

        testing_env!(get_context(alice()));
        contract.dr_final_arbitrator_finalize(U64(0), data_request::Outcome::Answer(AnswerType::String("b".to_string())));

        let request : DataRequest = contract.data_requests.get(0).unwrap();
        assert_eq!(request.resolution_windows.len(), 2);
        assert_eq!(request.finalized_outcome.unwrap(), data_request::Outcome::Answer(AnswerType::String("b".to_string())));
    }

    #[test]
    #[should_panic(expected = "Cannot stake on `DataRequest` 0 until settlement time 100")]
    fn dr_stake_before_settlement_time() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(100),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });

        contract.dr_stake(alice(), 10, StakeDataRequestArgs{
            id: U64(0),
            outcome: data_request::Outcome::Answer(AnswerType::String("b".to_string()))
        });

    }

    #[test]
    fn dr_tvl_increases() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);

        let outcome = data_request::Outcome::Answer(AnswerType::String("b".to_string()));
        contract.dr_stake(alice(), 10, StakeDataRequestArgs{
            id: U64(0),
            outcome
        });
    }

    #[test]
    fn dr_fixed_fee() {
        testing_env!(get_context(token()));
        let bob_requestor = RegistryEntry {
            interface_name: bob(),
            contract_entry: bob(),
            custom_fee: CustomFeeStakeArgs::Fixed(U128(15)),
            code_base_url: None,
        };
        let whitelist = Some(vec![bob_requestor, registry_entry(carol())]);
        let mut config = config();
        config.validity_bond = U128(2);
        let mut contract = Contract::new(whitelist, config);
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(
            data_request::AnswerType::String("a".to_string())
        ));

        let mut d = contract.data_requests.get(0).unwrap();
        assert_eq!(sum_claim_res(d.claim(alice())), 19);
    }

    #[test]
    fn dr_fixed_fee2() {
        testing_env!(get_context(token()));
        let bob_requestor = RegistryEntry {
            interface_name: bob(),
            contract_entry: bob(),
            custom_fee: CustomFeeStakeArgs::Fixed(U128(71)),
            code_base_url: None,
        };
        let whitelist = Some(vec![bob_requestor, registry_entry(carol())]);
        let mut config = config();
        config.validity_bond = U128(2);
        let mut contract = Contract::new(whitelist, config);
        contract.dr_new(bob(), 100, 5, NewDataRequestArgs{
            sources: Vec::new(),
            outcomes: Some(vec!["a".to_string(), "b".to_string()].to_vec()),
            challenge_period: U64(1500),
            settlement_time: U64(0),
            target_contract: target(),
            description: Some("a".to_string()),
            tags: None,
            data_type: data_request::DataRequestDataType::String,
            creator: bob(),
        });
        dr_finalize(&mut contract, data_request::Outcome::Answer(
            data_request::AnswerType::String("a".to_string())
        ));

        let mut d = contract.data_requests.get(0).unwrap();
        assert_eq!(sum_claim_res(d.claim(alice())), 75);
    }

    #[test]
    fn dr_get_methods() {
        testing_env!(get_context(token()));
        let whitelist = Some(vec![registry_entry(bob()), registry_entry(carol())]);
        let mut contract = Contract::new(whitelist, config());
        dr_new(&mut contract);
        dr_new(&mut contract);
        dr_new(&mut contract);
        
        assert_eq!(contract.get_latest_request().unwrap().id, 2);
        assert_eq!(contract.get_request_by_id(U64(1)).unwrap().id, 1);

        assert_eq!(contract.get_requests(U64(0), U64(1))[0].id, 0);
        assert_eq!(contract.get_requests(U64(1), U64(1)).len(), 1);
        assert_eq!(contract.get_requests(U64(1), U64(2)).len(), 2);
        assert_eq!(contract.get_requests(U64(0), U64(3)).len(), 3);
    }
}
