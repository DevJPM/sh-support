use std::{
    collections::{BTreeMap, HashMap},
    str
};

use cached::proc_macro::cached;
use contracts::{debug_ensures, debug_invariant};
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{policy::Policy, Context, Error};

#[derive(Default, Debug, Clone)]
pub(crate) struct DeckState {
    num_cards : usize,
    actual_decks : Vec<Vec<Policy>>
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
fn generate_internal(num_lib : usize, num_fasc : usize) -> DeckState {
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

#[debug_ensures(ret.iter().map(|(_k,v)|v).sum::<usize>() == decks.len())]
fn compute_window_histogram(
    decks : &Vec<Vec<Policy>>,
    window_size : usize
) -> BTreeMap<usize, usize> {
    decks
        .iter()
        .map(|deck| {
            deck.iter()
                .take(window_size)
                .filter(|p| **p == Policy::Liberal)
                .count()
        })
        .sorted()
        .group_by(|x| *x)
        .into_iter()
        .map(|(k, v)| (k, v.count()))
        .collect()
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

#[debug_invariant(_context.invariant())]
pub(crate) fn next(
    args : HashMap<String, Value>,
    _context : &mut Context
) -> Result<Option<String>, Error> {
    let deck_state = generate(&args)?;
    let pattern : String = args["pattern"].convert()?;

    let (num_lib_in_pattern, pattern_length, pattern) =
        parse_pattern(pattern, deck_state.num_cards, 0)?;

    let num_matching_decks = deck_state
        .actual_decks
        .iter()
        .filter(|d| {
            d.iter()
                .take(pattern_length)
                .filter(|p| **p == Policy::Liberal)
                .count()
                == num_lib_in_pattern
        })
        .count();

    let probability = (num_matching_decks as f64) / (deck_state.actual_decks.len() as f64);

    Ok(Some(format!(
        "There is a {:.1}% ({}/{}) chance for the claim pattern {} to match the next {} cards.",
        probability * 100.0,
        num_matching_decks,
        deck_state.actual_decks.len(),
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
