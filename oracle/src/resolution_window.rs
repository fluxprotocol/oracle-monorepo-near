use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::serde::{ Deserialize, Serialize };
use near_sdk::{ Balance, AccountId };
use near_sdk::collections::{ LookupMap };

use crate::types::*;
use crate::logger;

pub enum WindowStakeResult {
    Incorrect(Balance), // Round bonded outcome was correct
    Correct(CorrectStake), // Round bonded outcome was incorrect
    NoResult // Last / non-bonded window
}

pub struct CorrectStake {
    pub bonded_stake: Balance,
    pub user_stake: Balance,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ResolutionWindow {
    pub dr_id: u64,
    pub round: u16,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub bond_size: Balance,
    pub outcome_to_stake: LookupMap<Outcome, Balance>,
    pub user_to_outcome_to_stake: LookupMap<AccountId, LookupMap<Outcome, Balance>>,
    pub bonded_outcome: Option<Outcome>,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct ResolutionWindowSummary {
    pub round: u16,
    pub start_time: WrappedTimestamp, 
    pub end_time: WrappedTimestamp,
    pub bond_size: WrappedBalance,
    pub bonded_outcome: Option<Outcome>
}

impl ResolutionWindow {
    pub fn new(dr_id: u64, round: u16, prev_bond: Balance, challenge_period: u64, start_time: u64) -> Self {
        let new_resolution_window = Self {
            dr_id,
            round,
            start_time,
            end_time: start_time + challenge_period,
            bond_size: prev_bond * 2,
            outcome_to_stake: LookupMap::new(format!("ots{}:{}", dr_id, round).as_bytes().to_vec()),
            user_to_outcome_to_stake: LookupMap::new(format!("utots{}:{}", dr_id, round).as_bytes().to_vec()),
            bonded_outcome: None
        };

        logger::log_resolution_window(&new_resolution_window);
        return new_resolution_window;
    }

    // @returns amount to refund users because it was not staked
    pub fn stake(&mut self, sender: AccountId, outcome: Outcome, amount: Balance) -> Balance {
        let stake_on_outcome = self.outcome_to_stake.get(&outcome).unwrap_or(0);
        let mut user_to_outcomes = self.user_to_outcome_to_stake
            .get(&sender)
            .unwrap_or(LookupMap::new(format!("utots:{}:{}:{}", self.dr_id, self.round, sender).as_bytes().to_vec()));
        let user_stake_on_outcome = user_to_outcomes.get(&outcome).unwrap_or(0);

        let stake_open = self.bond_size - stake_on_outcome;
        let unspent = if amount > stake_open {
            amount - stake_open
        } else {
            0
        };

        let staked = amount - unspent;

        let new_stake_on_outcome = stake_on_outcome + staked;
        self.outcome_to_stake.insert(&outcome, &new_stake_on_outcome);
        logger::log_outcome_to_stake(self.dr_id, self.round, &outcome, new_stake_on_outcome);

        let new_user_stake_on_outcome = user_stake_on_outcome + staked;
        user_to_outcomes.insert(&outcome, &new_user_stake_on_outcome);
        self.user_to_outcome_to_stake.insert(&sender, &user_to_outcomes);

        logger::log_user_stake(self.dr_id, self.round, &sender, &outcome, new_user_stake_on_outcome);
        logger::log_stake_transaction(&sender, &self, amount, unspent, &outcome);

        // If this stake fills the bond set final outcome which will trigger a new resolution_window to be created
        if new_stake_on_outcome == self.bond_size {
            self.bonded_outcome = Some(outcome);
            logger::log_resolution_window(&self);
        }

        unspent
    }

    // @returns amount to refund users because it was not staked
    pub fn unstake(&mut self, sender: AccountId, outcome: Outcome, amount: Balance) -> Balance {
        assert!(self.bonded_outcome.is_none() || self.bonded_outcome.as_ref().unwrap() != &outcome, "Cannot withdraw from bonded outcome");
        let mut user_to_outcomes = self.user_to_outcome_to_stake
            .get(&sender)
            .unwrap_or(LookupMap::new(format!("utots:{}:{}:{}", self.dr_id, self.round, sender).as_bytes().to_vec()));
        let user_stake_on_outcome = user_to_outcomes.get(&outcome).unwrap_or(0);
        assert!(user_stake_on_outcome >= amount, "{} has less staked on this outcome ({}) than unstake amount", sender, user_stake_on_outcome);

        let stake_on_outcome = self.outcome_to_stake.get(&outcome).unwrap_or(0);

        let new_stake_on_outcome = stake_on_outcome - amount;
        self.outcome_to_stake.insert(&outcome, &new_stake_on_outcome);
        logger::log_outcome_to_stake(self.dr_id, self.round, &outcome, new_stake_on_outcome);

        let new_user_stake_on_outcome = user_stake_on_outcome - amount;
        user_to_outcomes.insert(&outcome, &new_user_stake_on_outcome);
        self.user_to_outcome_to_stake.insert(&sender, &user_to_outcomes);
        logger::log_user_stake(self.dr_id, self.round, &sender, &outcome, new_user_stake_on_outcome);
        logger::log_unstake_transaction(&sender, &self, amount, &outcome);

        amount
    }

    pub fn claim_for(&mut self, account_id: AccountId, final_outcome: &Outcome) -> WindowStakeResult {
        // Check if there is a bonded outcome, if there is none it means it can be ignored in payout calc since it can only be the final unsuccessful window
        match &self.bonded_outcome {
            Some(bonded_outcome) => {
                // If the bonded outcome for this window is equal to the finalized outcome the user's stake in this window and the total amount staked should be returned (which == `self.bond_size`)
                if bonded_outcome == final_outcome {
                    WindowStakeResult::Correct(CorrectStake {
                        bonded_stake: self.bond_size,
                        // Get the users stake in this outcome for this window
                        user_stake:  match &mut self.user_to_outcome_to_stake.get(&account_id) {
                            Some(outcome_to_stake) => {
                                outcome_to_stake.remove(&bonded_outcome).unwrap_or(0)
                            },
                            None => 0
                        }
                    })
                // Else if the bonded outcome for this window is not equal to the finalized outcome the user's stake in this window only the total amount that was staked on the incorrect outcome should be returned
                } else {
                    WindowStakeResult::Incorrect(self.bond_size)
                }
            },
            None => WindowStakeResult::NoResult // Return `NoResult` for non-bonded window
        }
    }
}