use std::fmt;

use crate::{PlayerID, secret_role::SecretRole};

#[derive(Copy, Clone, Debug)]
pub(crate) enum Information {
    ConfirmedNotHitler(PlayerID),
    PolicyConflict(PlayerID, PlayerID),
    LiberalInvestigation {
        investigator : PlayerID,
        investigatee : PlayerID
    },
    FascistInvestigation {
        investigator : PlayerID,
        investigatee : PlayerID
    },
    HardFact(PlayerID, SecretRole)
}

impl fmt::Display for Information {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Information::ConfirmedNotHitler(pid) => {
                write!(f, "Player {pid} is confirmed to not be Hitler.")
            },
            Information::PolicyConflict(left, right) => {
                write!(
                    f,
                    "Player {left} is in a policy-based conflict with player {right}."
                )
            },
            Information::LiberalInvestigation {
                investigator,
                investigatee
            } => write!(
                f,
                "Player {investigator} investigated player {investigatee} and claimed to have \
                 found a liberal."
            ),
            Information::FascistInvestigation {
                investigator,
                investigatee
            } => write!(
                f,
                "Player {investigator} investigated player {investigatee} and claimed to have \
                 found a fascist."
            ),
            Information::HardFact(pid, role) => write!(f, "Player {pid} is known to be {role}.")
        }
    }
}