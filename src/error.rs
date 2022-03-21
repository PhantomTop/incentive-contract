use cosmwasm_std::{StdError, Uint128};
use cw_utils::{Expiration, Scheduled};
use hex::FromHexError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hex(#[from] FromHexError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("InvalidInput")]
    InvalidInput {},

    #[error("Not reward or lp token")]
    UnacceptableToken {},

    #[error("Not enough lp token")]
    NotEnoughLp {},

    #[error("No Reward")]
    NoReward {},

    #[error("No Staked")]
    NoStaked {},

    #[error("Not enough Reward")]
    NotEnoughReward { },

    #[error("Already claimed")]
    Claimed {},

    #[error("Wrong length")]
    WrongLength {},

    #[error("Map2List failed")]
    Map2ListFailed {},

    #[error("Cannot migrate from different contract type: {previous_contract}")]
    CannotMigrate { previous_contract: String },

    #[error("Airdrop stage {stage} expired at {expiration}")]
    StageExpired { stage: u8, expiration: Expiration },

    #[error("Airdrop stage {stage} begins at {start}")]
    StageNotBegun { stage: u8, start: Scheduled },

    #[error("Count {count}")]
    Count { count: u64 },
}
