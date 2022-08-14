use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, str
};

use cached::proc_macro::cached;
use contracts::{debug_ensures, debug_invariant};
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{
    players::{ElectionResult, ElectionResult::*},
    policy::Policy,
    Context, Error, PlayerID
};

#[derive(Default, Debug, Clone)]
pub(crate) struct DeckState {
    pub(crate) num_cards : usize,
    pub(crate) actual_decks : Vec<Vec<Policy>>
}

impl DeckState {
    pub(crate) fn invariant(&self) -> bool {
        self.actual_decks.iter().all(|d| d.len() == self.num_cards)
            && self.actual_decks.iter().all_unique()
    }
}

fn generate(args : &HashMap<String, Value>) -> Result<DeckState, Error> {
    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    Ok(generate_internal(num_lib, num_fasc))
}

#[cached]
#[debug_ensures(ret.invariant())]
pub(crate) fn generate_internal(num_lib : usize, num_fasc : usize) -> DeckState {
    let num_cards = num_lib + num_fasc;

    DeckState {
        num_cards,
        actual_decks : (0..num_cards)
            .into_iter()
            .combinations(num_lib)
            .map(|vlib| {
                let mut out = vec![Policy::Fascist; num_cards];
                vlib.iter().for_each(|i| out[*i] = Policy::Liberal);
                out
            })
            .collect_vec()
    }
}

#[debug_invariant(_context.invariant())]
pub(crate) fn dist(
    args : HashMap<String, Value>,
    _context : &mut Context
) -> Result<Option<String>, Error> {
    let deck_state = generate(&args)?;
    let window_size : usize = args["window_size"].convert()?;

    if window_size > deck_state.num_cards {
        return Err(Error::TooLongPatternError {
            have : deck_state.num_cards,
            requested : window_size
        });
    }

    let histogram = compute_window_histogram(&deck_state.actual_decks, window_size);

    let deck_count = deck_state.actual_decks.len();

    let out_text = histogram
        .into_iter()
        .map(|(key, value)| {
            (
                format!(
                    "{}{}",
                    Policy::Fascist.to_string().repeat(window_size - key),
                    Policy::Liberal.to_string().repeat(key)
                ),
                value
            )
        })
        .map(|(key, value)| {
            format!(
                "{}: {:.1}% ({}/{})",
                key,
                value as f64 / deck_count as f64 * 100.0,
                value,
                deck_count
            )
        })
        .join("\n");

    Ok(Some(out_text))
}

//#[debug_ensures(ret.iter().map(|(_k,v)|v).sum::<usize>() == decks.len())]
fn compute_window_histogram(
    decks : &Vec<Vec<Policy>>,
    window_size : usize
) -> BTreeMap<usize, usize> {
    decks
        .iter()
        .map(|d| count_policies(d, 0, window_size, Policy::Liberal))
        .sorted()
        .group_by(|x| *x)
        .into_iter()
        .map(|(k, v)| (k, v.count()))
        .collect()
}

fn count_policies(
    deck : &Vec<Policy>,
    offset : usize,
    window_size : usize,
    policy : Policy
) -> usize {
    deck.iter()
        .skip(offset)
        .take(window_size)
        .filter(|p| **p == policy)
        .count()
}

pub(crate) fn parse_pattern(
    pattern : String,
    max_pattern_length : usize,
    min_pattern_length : usize
) -> Result<(usize, usize, Vec<Policy>), Error> {
    let pattern : Result<Vec<Policy>, Error> = pattern
        .into_bytes()
        .into_iter()
        .map(|b| str::from_utf8(&[b])?.parse::<Policy>())
        .collect();
    let mut pattern = pattern?;
    pattern.sort();
    let pattern = pattern;

    let pattern_length = pattern.len();

    if pattern_length > max_pattern_length {
        return Err(Error::TooLongPatternError {
            have : max_pattern_length,
            requested : pattern_length
        });
    }
    if pattern_length < min_pattern_length {
        return Err(Error::TooShortPatternError {
            have : max_pattern_length,
            requested : pattern_length
        });
    }

    let num_lib_in_pattern = pattern.iter().filter(|p| **p == Policy::Liberal).count();

    Ok((num_lib_in_pattern, pattern_length, pattern))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FilterResult {
    pub num_matching : usize,
    pub num_checked : usize
}

impl FilterResult {
    pub fn probability(&self) -> f64 { self.num_matching as f64 / self.num_checked as f64 }

    pub fn none(out_of : usize) -> Self {
        FilterResult {
            num_matching : 0,
            num_checked : out_of
        }
    }
}

impl fmt::Display for FilterResult {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.1}% ({}/{})",
            self.probability() * 100.0,
            self.num_matching,
            self.num_checked
        )
    }
}

#[cached]
fn hard_facted_complex_card_counter(
    num_total_lib : usize,
    num_total_fasc : usize,
    hard_facts : Vec<ElectionResult>,
    hard_confirmed_libs : BTreeSet<PlayerID>
) -> DeckState {
    let decks = generate_internal(num_total_lib, num_total_fasc);
    DeckState {
        num_cards : decks.num_cards,
        actual_decks : decks
            .actual_decks
            .into_iter()
            .filter(|d| {
                hard_facts
                    .iter()
                    .scan(0, |offset, er| {
                        let (drawn, _discarded) = er.cards_total_drawn_discarded();
                        let blue_count = count_policies(d, *offset, drawn, Policy::Liberal);
                        let red_count = count_policies(d, *offset, drawn, Policy::Fascist);
                        let drawn_blue = blue_count >= er.passed_blues();
                        let drawn_red = red_count >= 1 - er.passed_blues();
                        let good_liberals = match er {
                            Election(eg) => {
                                let president = !hard_confirmed_libs.contains(&eg.president)
                                    || eg.president_claimed_blues == blue_count;
                                let chancellor_blue = !hard_confirmed_libs.contains(&eg.chancellor)
                                    || eg.chancellor_claimed_blues <= blue_count;
                                let chancellor_red = !hard_confirmed_libs.contains(&eg.chancellor)
                                    || 2 - eg.chancellor_claimed_blues <= red_count;
                                president && chancellor_blue && chancellor_red
                            },
                            TopDeck(_, _) => true
                        };
                        *offset += drawn;
                        Some(drawn_blue && drawn_red && good_liberals)
                    })
                    .all(|x| x)
            })
            .collect()
    }
}

pub(crate) fn complex_card_counter(
    num_total_lib : usize,
    num_total_fasc : usize,
    hard_facts : &[&ElectionResult],
    hypotheses : &[ElectionResult],
    legal_follow_on_sets : &Vec<Option<BTreeSet<usize>>>,
    hard_confirmed_liberals : &BTreeSet<usize>,
    path_assumed_liberals : &BTreeSet<usize>,
    new_hypothesis : &ElectionResult
) -> FilterResult {
    let decks = hard_facted_complex_card_counter(
        num_total_lib,
        num_total_fasc,
        hard_facts.iter().map(|er| (*er).clone()).collect(),
        hard_confirmed_liberals.clone()
    );
    let decks = DeckState {
        num_cards : decks.num_cards,
        actual_decks : decks
            .actual_decks
            .into_iter()
            .filter(|d| {
                hard_facts
                    .iter()
                    .enumerate()
                    .scan(0, |offset, (idx, er)| {
                        let (drawn, _discarded) = er.cards_total_drawn_discarded();
                        let blue_count = count_policies(d, *offset, drawn, Policy::Liberal);
                        let red_count = count_policies(d, *offset, drawn, Policy::Fascist);
                        let follow_on = legal_follow_on_sets
                            .get(idx)
                            .map(|seto| {
                                seto.as_ref()
                                    .map(|set| set.contains(&blue_count))
                                    .unwrap_or(true)
                            })
                            .unwrap_or(true);
                        let good_liberals = match er {
                            Election(eg) => {
                                let president = !path_assumed_liberals.contains(&eg.president)
                                    || er.seen_blues() == blue_count; // need to use seen_blues() here because of peek-and-burns
                                let chancellor_blue = !path_assumed_liberals
                                    .contains(&eg.chancellor)
                                    || eg.chancellor_claimed_blues <= blue_count;
                                let chancellor_red = !path_assumed_liberals
                                    .contains(&eg.chancellor)
                                    || 2 - eg.chancellor_claimed_blues <= red_count;
                                president && chancellor_blue && chancellor_red
                            },
                            TopDeck(_, _) => true
                        };
                        *offset += drawn;
                        Some(good_liberals && follow_on)
                    })
                    .all(|x| x)
            })
            .filter(|d| {
                hypotheses
                    .iter()
                    .scan(0, |offset, er| {
                        let (drawn, _discarded) = er.cards_total_drawn_discarded();
                        let ret =
                            count_policies(d, *offset, drawn, Policy::Liberal) == er.seen_blues();
                        *offset += drawn;
                        Some(ret)
                    })
                    .all(|x| x)
            })
            .collect()
    };

    let target_offset = hypotheses
        .iter()
        .map(|er| er.cards_total_drawn_discarded().0)
        .sum();

    FilterResult {
        num_matching : decks
            .actual_decks
            .iter()
            .filter(|d| {
                count_policies(
                    d,
                    target_offset,
                    new_hypothesis.cards_total_drawn_discarded().0,
                    Policy::Liberal
                ) == new_hypothesis.seen_blues()
            })
            .count(),
        num_checked : decks.actual_decks.len()
    }
}

#[cached]
pub(crate) fn next_blues_count(
    num_total_lib : usize,
    num_total_fasc : usize,
    window_size : usize,
    desired_blues_in_window : usize,
    guaranteed_blues_in_window : usize,
    guaranteed_reds_in_window : usize
) -> FilterResult {
    let decks = generate_internal(num_total_lib, num_total_fasc);
    let decks = DeckState {
        num_cards : decks.num_cards,
        actual_decks : decks
            .actual_decks
            .into_iter()
            .filter(|d| {
                count_policies(d, 0, window_size, Policy::Liberal) >= guaranteed_blues_in_window
                    && count_policies(d, 0, window_size, Policy::Fascist)
                        >= guaranteed_reds_in_window
            })
            .collect()
    };

    FilterResult {
        num_matching : decks
            .actual_decks
            .iter()
            .filter(|d| {
                count_policies(d, 0, window_size, Policy::Liberal) == desired_blues_in_window
            })
            .count(),
        num_checked : decks.actual_decks.len()
    }
}

#[debug_invariant(_context.invariant())]
pub(crate) fn next(
    args : HashMap<String, Value>,
    _context : &mut Context
) -> Result<Option<String>, Error> {
    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;
    let pattern : String = args["pattern"].convert()?;

    let (num_lib_in_pattern, pattern_length, pattern) =
        parse_pattern(pattern, num_lib + num_lib, 0)?;

    let analysis = next_blues_count(num_lib, num_fasc, pattern_length, num_lib_in_pattern, 0, 0);

    Ok(Some(format!(
        "There is a {analysis} chance for the claim pattern {} to match the next {} cards.",
        pattern.iter().map(|p| p.to_string()).join(""),
        pattern_length
    )))
}

#[debug_invariant(_context.invariant())]
pub(crate) fn debug_decks(
    args : HashMap<String, Value>,
    _context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        generate(&args)?
            .actual_decks
            .iter()
            .map(|vpol| vpol.iter().map(|pol| format!("{}", pol)).join(""))
            .join("\n")
    ))
}
