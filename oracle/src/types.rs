use near_sdk::json_types::{U64, U128};
use near_sdk::borsh::{ self, BorshDeserialize, BorshSerialize };
use near_sdk::serde::{ Deserialize, Serialize };

/// Raw type for timestamp in nanoseconds
pub type Timestamp = u64;
pub type WrappedTimestamp = U64;
pub type Duration = u64;
pub type WrappedBalance = U128;

pub struct ClaimRes {
    pub payment_token_payout: u128,
    pub stake_token_payout: u128
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct AnswerNumberType {
    pub value: U128,
    pub multiplier: U128,
    pub negative: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum AnswerType {
    Number(AnswerNumberType),
    String(String)
}

#[derive(BorshSerialize, BorshDeserialize, Deserialize, Serialize, Debug, PartialEq, Clone)]
pub enum Outcome {
    Answer(AnswerType),
    Invalid
}
