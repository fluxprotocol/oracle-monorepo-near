use crate::*;
use flux_sdk::{
    consts::{MAX_SOURCES, MAX_TAGS, MIN_OUTCOMES, MIN_PERIOD_MULTIPLIER},
    data_request::NewDataRequestArgs,
};

impl Contract {
    pub fn dr_validate(&self, data_request: &NewDataRequestArgs) {
        let config = self.get_config();
        let challenge_period: u64 = data_request.challenge_period.into();
        let default_challenge_window_duration: u64 =
            config.default_challenge_window_duration.into();
        let min_initial_challenge_window_duration: u64 =
            config.min_initial_challenge_window_duration.into();

        assert!(
            (data_request.description.is_none()
                && data_request.sources.as_ref().unwrap_or(&vec![]).len() as u8 != 0)
                || data_request.description.is_some(),
            "Description should be filled when no sources are given"
        );
        assert!(
            data_request.sources.as_ref().unwrap_or(&vec![]).len() as u8 <= MAX_SOURCES,
            "Too many sources provided, max sources is: {}",
            MAX_SOURCES
        );
        assert!(
            challenge_period >= u64::from(min_initial_challenge_window_duration),
            "Challenge shorter than minimum challenge period of {}",
            min_initial_challenge_window_duration
        );
        assert!(
            challenge_period <= default_challenge_window_duration * MIN_PERIOD_MULTIPLIER,
            "Challenge period exceeds maximum challenge period of {}",
            default_challenge_window_duration * MIN_PERIOD_MULTIPLIER
        );
        assert!(
            data_request.tags.len() == 0 || data_request.tags.len() as u8 <= MAX_TAGS,
            "Too many tags provided, max tags is: {}",
            MAX_TAGS
        );
        assert!(
            data_request.outcomes.is_none()
                || data_request.outcomes.as_ref().unwrap().len() as u8 <= config.max_outcomes
                    && data_request.outcomes.as_ref().unwrap().len() as u8 >= MIN_OUTCOMES,
            "Invalid outcome list either exceeds min of: {} or max of {}",
            MIN_OUTCOMES,
            config.max_outcomes
        );
    }
}
