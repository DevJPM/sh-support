use std::{
    collections::BTreeMap,
    fmt::{self},
    io::{self, BufRead, Write},
    ops::RangeInclusive
};

use cached::proc_macro::cached;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    policy::Policy,
    secret_role::SecretRole,
    PlayerID
};

use super::{PlayerInfo, PresidentialAction, PresidentialAction::*};

#[derive(Debug, Serialize, Deserialize)]
#[readonly::make]
pub(crate) struct GameConfiguration {
    pub table_size : usize,
    pub num_regular_fascists : usize,
    pub initial_liberal_deck_policies : usize,
    pub initial_fascist_deck_policies : usize,
    pub initial_placed_liberal_policies : usize,
    pub initial_placed_fascist_policies : usize,
    pub fascist_board_configuration : [PresidentialAction; 5],
    pub hitler_zone_passed_fascist_policies : usize,
    pub veto_zone_passed_fascist_policies : usize
}

impl fmt::Display for GameConfiguration {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{:?}", self) }
}

const SMALL_BOARD : [PresidentialAction; 5] = [
    NoAction,
    NoAction,
    TopDeckPeek([Policy::Liberal, Policy::Liberal, Policy::Liberal]),
    Kill(0),
    Kill(0)
];
const MEDIUM_BOARD : [PresidentialAction; 5] = [
    NoAction,
    Investigation(0, Policy::Liberal),
    SpecialElection(0),
    Kill(0),
    Kill(0)
];
const LARGE_BOARD : [PresidentialAction; 5] = [
    Investigation(0, Policy::Liberal),
    Investigation(0, Policy::Liberal),
    SpecialElection(0),
    Kill(0),
    Kill(0)
];

impl GameConfiguration {
    pub(crate) fn new_standard(table_size : usize, rebalanced : bool) -> Result<Self> {
        Ok(GameConfiguration {
            table_size,
            hitler_zone_passed_fascist_policies : 3,
            veto_zone_passed_fascist_policies : 5,
            num_regular_fascists : (table_size - 1) / 2 - 1,
            initial_liberal_deck_policies : 6,
            initial_fascist_deck_policies : if rebalanced && matches!(table_size, 6 | 7 | 9) {
                10
            }
            else {
                11
            },
            initial_placed_liberal_policies : 0,
            initial_placed_fascist_policies : if rebalanced && table_size == 6 { 1 } else { 0 },
            fascist_board_configuration : match table_size {
                5 | 6 => SMALL_BOARD,
                7 | 8 => MEDIUM_BOARD,
                9 | 10 => LARGE_BOARD,
                _ => return Err(Error::BadPlayerCount(table_size))
            }
        })
    }

    pub(crate) fn invariant(&self) -> bool {
        self.num_regular_fascists < self.table_size / 2
            // these bounds aren't inherent, they're just a consequence of SecretHitler.io's restrictions
            && matches!(self.table_size, 5..=10)
            && matches!(self.initial_fascist_deck_policies, 10..=19)
            && matches!(self.initial_liberal_deck_policies, 5..=8)
            && matches!(self.initial_placed_liberal_policies, 0..=2)
            && matches!(self.initial_placed_fascist_policies, 0..=2)
            && matches!(self.hitler_zone_passed_fascist_policies, 1..=5)
            && matches!(self.veto_zone_passed_fascist_policies, 1..=5)
    }

    pub(crate) fn generate_assignments(&self) -> Vec<BTreeMap<PlayerID, SecretRole>> {
        generate_assignments_cached(self.table_size, self.num_regular_fascists)
    }

    pub(crate) fn generate_default_info(&self) -> BTreeMap<usize, PlayerInfo> {
        generate_default_info_cached(self.table_size)
    }

    pub fn interactively_ask_for_configuration() -> Self {
        let mut table_size = 7;
        ask_for_value(&mut table_size, "seated players", 5..=10);

        let mut config = GameConfiguration::new_standard(table_size, false).unwrap();

        ask_for_value(
            &mut config.num_regular_fascists,
            "non-hitler fascists",
            1..=((table_size - 1) / 2 - 1)
        );

        ask_for_value(
            &mut config.initial_liberal_deck_policies,
            "liberal policies in the deck",
            5..=8
        );

        ask_for_value(
            &mut config.initial_fascist_deck_policies,
            "fascist policies in the deck",
            10..=19
        );

        ask_for_value(
            &mut config.initial_placed_liberal_policies,
            "liberal policies initially on the board",
            0..=2
        );

        ask_for_value(
            &mut config.initial_placed_fascist_policies,
            "fascist policies initially on the board",
            0..=2
        );

        ask_for_value(
            &mut config.hitler_zone_passed_fascist_policies,
            "fascist policies having to be on the board to unlock a hitler chancellor election to \
             mean a fascist win",
            1..=5
        );

        ask_for_value(
            &mut config.veto_zone_passed_fascist_policies,
            "fascist policies to unlock the veto power",
            1..=5
        );

        config.fascist_board_configuration = ask_for_board();

        config
    }
}

#[cached]
fn generate_default_info_cached(table_size : usize) -> BTreeMap<usize, PlayerInfo> {
    (1..=table_size)
        .into_iter()
        .map(|pid| {
            (
                pid,
                PlayerInfo {
                    seat : pid,
                    name : String::new()
                }
            )
        })
        .collect()
}

#[cached]
fn generate_assignments_cached(
    table_size : usize,
    num_regular_fascists : usize
) -> Vec<BTreeMap<PlayerID, SecretRole>> {
    (0..table_size - 1)
        .into_iter()
        .combinations(num_regular_fascists)
        .flat_map(move |fasc_pos| {
            (0..table_size).into_iter().map(move |hitler_pos| {
                (
                    hitler_pos,
                    fasc_pos
                        .iter()
                        .map(|fp| {
                            if *fp >= hitler_pos {
                                fp + 1
                            }
                            else {
                                *fp
                            }
                        })
                        .collect_vec()
                )
            })
        })
        .map(|(hitler_pos, fascist_pos)| {
            let mut out = vec![SecretRole::Liberal; table_size];
            out[hitler_pos] = SecretRole::Hitler;
            fascist_pos
                .iter()
                .for_each(|i| out[*i] = SecretRole::RegularFascist);
            out.into_iter()
                .enumerate()
                .map(|(pos, role)| (pos + 1, role))
                .collect::<BTreeMap<_, _>>()
        })
        .collect_vec()
}

impl Default for GameConfiguration {
    fn default() -> Self { Self::new_standard(7, false).unwrap() }
}

fn ask_for_board() -> [PresidentialAction; 5] {
    let mut out = [NoAction; 5];

    for i in 1..=5 {
        out[i - 1] = ask_for_action(i);
    }

    out
}

fn ask_for_action(index : usize) -> PresidentialAction {
    loop {
        println!(
            "Please select the presidential action you'd like to have happen for the fascist \
             policy #{index}:"
        );
        println!("<1> no action");
        println!("<2> the president kills a player");
        println!("<3> the president investigates a player's party membership");
        println!("<4> the president reveals their party membership to a player they select");
        println!("<5> the president peeks at the next three policies");
        println!("<6> the president gets to select the next presidential candidate");
        println!("<7> the president peeks at the next policy and may discard it");
        print!("please enter a number:   ");
        io::stdout().flush().expect("flush failed!");

        // get user input
        let mut locked_stdin = io::stdin().lock();
        let mut output = String::new();
        let value = match locked_stdin.read_line(&mut output) {
            Ok(_) => output.trim().to_string(),
            Err(_) => continue
        };

        let value = match value.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                println!("Failed to understand this input as an integer.");
                continue;
            }
        };

        if value < 1 {
            println!("This value is too small.");
            continue;
        }
        if value > 7 {
            println!("This value is too large.");
            continue;
        }

        return match value {
            1 => NoAction,
            2 => Kill(0),
            3 => Investigation(0, Policy::Liberal),
            4 => RevealParty(0, Policy::Liberal),
            5 => TopDeckPeek([Policy::Liberal, Policy::Liberal, Policy::Liberal]),
            6 => SpecialElection(0),
            7 => PeekAndBurn(Policy::Liberal, false, Default::default()),
            _ => continue
        };
    }
}

fn ask_for_value(io : &mut usize, text : &str, valid_range : RangeInclusive<usize>) {
    loop {
        print!(
            "Please enter the number of {text} (valid values: {} - {}, default: {io}):   ",
            valid_range.start(),
            valid_range.end()
        );
        io::stdout().flush().expect("flush failed!");

        // get user input
        let mut locked_stdin = io::stdin().lock();
        let mut output = String::new();
        let mut value = match locked_stdin.read_line(&mut output) {
            Ok(_) => output.trim().to_string(),
            Err(_) => continue
        };

        if value.is_empty() {
            value = format!("{io}");
        }

        let value = match value.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                println!("Failed to understand this input as an integer.");
                continue;
            }
        };

        if value < *valid_range.start() {
            println!("This value is too small.");
            continue;
        }
        if value > *valid_range.end() {
            println!("This value is too large.");
            continue;
        }
        *io = value;
        return;
    }
}
