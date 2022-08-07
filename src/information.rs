use itertools::Itertools;

use crate::{
    players::{PlayerFormatable, PlayerInfos},
    secret_role::SecretRole,
    PlayerID, PlayerManager
};

#[derive(Clone, Debug)]
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
    HardFact(PlayerID, SecretRole),
    AtLeastOneFascist(Vec<PlayerID>)
}

impl PlayerFormatable for Information {
    fn format(&self, player_info : &PlayerInfos) -> String {
        match self {
            Information::ConfirmedNotHitler(pid) => {
                format!(
                    "Player {} is confirmed to not be Hitler.",
                    player_info.format_name(*pid)
                )
            },
            Information::PolicyConflict(left, right) => {
                format!(
                    "Player {} is in a policy-based conflict with player {}.",
                    player_info.format_name(*left),
                    player_info.format_name(*right)
                )
            },
            Information::LiberalInvestigation {
                investigator,
                investigatee
            } => format!(
                "Player {} investigated player {} and claimed to have found a liberal.",
                player_info.format_name(*investigator),
                player_info.format_name(*investigatee)
            ),
            Information::FascistInvestigation {
                investigator,
                investigatee
            } => format!(
                "Player {} investigated player {} and claimed to have found a fascist.",
                player_info.format_name(*investigator),
                player_info.format_name(*investigatee)
            ),
            Information::HardFact(pid, role) => format!(
                "Player {} is known to be {role}.",
                player_info.format_name(*pid)
            ),
            Information::AtLeastOneFascist(suspicious_players) => format!(
                "At least one of {} is a confirmed fascist.",
                suspicious_players
                    .iter()
                    .map(|pid| format!("Player {}", player_info.format_name(*pid)))
                    .join(", ")
            )
        }
    }
}
