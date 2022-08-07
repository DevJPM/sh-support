use std::{fmt, str};

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum Policy {
    Liberal,
    Fascist
}

impl PartialOrd for Policy {
    fn partial_cmp(&self, other : &Self) -> Option<std::cmp::Ordering> {
        Some(Self::cmp(self, other))
    }
}

impl Ord for Policy {
    fn cmp(&self, other : &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Policy::Fascist, Policy::Liberal) => std::cmp::Ordering::Less,
            (Policy::Liberal, Policy::Fascist) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal
        }
    }
}

impl str::FromStr for Policy {
    type Err = Error;

    fn from_str(s : &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "f" | "r" => Ok(Policy::Fascist),
            "l" | "b" => Ok(Policy::Liberal),
            _ => Err(Error::ParsePolicyError(s.to_owned()))
        }
    }
}

impl fmt::Display for Policy {
    fn fmt(&self, f : &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Policy::Liberal => write!(f, "B"),
            Policy::Fascist => write!(f, "R")
        }
    }
}
