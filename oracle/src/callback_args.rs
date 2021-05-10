use crate::*;
use near_sdk::serde::{ Serialize, Deserialize };

const MAX_SOURCES: u8 = 8;
const MIN_OUTCOMES: u8 = 2;
const MIN_PERIOD_MULTIPLIER: u64 = 3;

#[derive(Serialize, Deserialize)]
pub struct NewDataRequestArgs {
    pub sources: Vec<data_request::Source>,
    pub description: Option<String>,
    pub outcomes: Option<Vec<String>>,
    pub challenge_period: Timestamp,
    pub settlement_time: U64,
    pub target_contract: AccountId,
}

impl Contract {
    pub fn dr_validate(&self, data_request: &NewDataRequestArgs) {
        let config = self.get_config();

        assert!((data_request.description.is_none() && data_request.sources.len() as u8 != 0) || data_request.description.is_some(), "Description should be filled when no sources are given");
        assert!(data_request.sources.len() as u8 <= MAX_SOURCES, "Too many sources provided, max sources is: {}", MAX_SOURCES);
        assert!(data_request.challenge_period >= config.min_initial_challenge_window_duration, "Challenge shorter than minimum challenge period of {}", config.min_initial_challenge_window_duration);
        assert!(data_request.challenge_period <= config.default_challenge_window_duration * MIN_PERIOD_MULTIPLIER, "Challenge period exceeds maximum challenge period of {}", config.default_challenge_window_duration * MIN_PERIOD_MULTIPLIER);
        assert!(
            data_request.outcomes.is_none() ||
            data_request.outcomes.as_ref().unwrap().len() as u8 <= config.max_outcomes &&
            data_request.outcomes.as_ref().unwrap().len() as u8 >= MIN_OUTCOMES,
            "Invalid outcome list either exceeds min of: {} or max of {}",
            MIN_OUTCOMES,
            config.max_outcomes
        );
    }
}

#[derive(Serialize, Deserialize)]
pub struct StakeDataRequestArgs {
    pub id: U64,
    pub outcome: data_request::Outcome,
}

#[derive(Serialize, Deserialize)]
pub struct ChallengeDataRequestArgs {
    pub id: U64,
    pub answer: data_request::Outcome,
}
