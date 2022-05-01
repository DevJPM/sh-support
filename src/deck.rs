use std::{
    collections::{BTreeMap, HashMap},
    str
};

use contracts::{debug_ensures, debug_invariant};
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{policy::Policy, Context, Error};

#[derive(Default, Debug)]
pub(crate) struct DeckState {
    num_cards : usize,
    current_decks : Vec<Vec<Policy>>
}

impl DeckState {
    pub(crate) fn invariant(&self) -> bool {
        self.current_decks.iter().all(|d| d.len() == self.num_cards)
            && self.current_decks.iter().all_unique()
    }
}

#[debug_invariant(context.invariant())]
pub(crate) fn generate(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut deck_state = &mut context.deck_state;
    deck_state.current_decks.clear();
    deck_state.num_cards = 0;

    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    let deck_size = num_lib + num_fasc;

    deck_state.num_cards = deck_size;
    deck_state.current_decks = (0..deck_size)
        .into_iter()
        .combinations(num_lib)
        .map(|vlib| {
            let mut out = vec![Policy::Fascist; deck_size];
            vlib.iter().for_each(|i| out[*i] = Policy::Liberal);
            out
        })
        .collect_vec();

    Ok(Some(format!(
        "Successfully generated {} decks with {} liberal and {} fascist policies each.",
        deck_state.current_decks.len(),
        num_lib,
        num_fasc
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn dist(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut deck_state = &mut context.deck_state;
    let window_size : usize = args["window_size"].convert()?;

    if window_size > deck_state.num_cards {
        return Err(Error::TooLongPatternError {
            have : deck_state.num_cards,
            requested : window_size
        });
    }

    let histogram = compute_window_histogram(&deck_state.current_decks, window_size);

    let deck_count = deck_state.current_decks.len();

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

#[debug_invariant(context.invariant())]
pub(crate) fn next(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut deck_state = &mut context.deck_state;
    let pattern : String = args["pattern"].convert()?;

    let pattern : Result<Vec<Policy>, Error> = pattern
        .into_bytes()
        .into_iter()
        .map(|b| str::from_utf8(&[b])?.parse::<Policy>())
        .collect();
    let mut pattern = pattern?;
    pattern.sort();
    let pattern = pattern;

    let pattern_length = pattern.len();

    if pattern_length > deck_state.num_cards {
        return Err(Error::TooLongPatternError {
            have : deck_state.num_cards,
            requested : pattern_length
        });
    }

    let num_lib_in_pattern = pattern.iter().filter(|p| **p == Policy::Liberal).count();

    let num_matching_decks = deck_state
        .current_decks
        .iter()
        .filter(|d| {
            d.iter()
                .take(pattern_length)
                .filter(|p| **p == Policy::Liberal)
                .count()
                == num_lib_in_pattern
        })
        .count();

    let probability = (num_matching_decks as f64) / (deck_state.current_decks.len() as f64);

    Ok(Some(format!(
        "There is a {:.1}% ({}/{}) chance for the claim pattern {} to match the next {} cards.",
        probability * 100.0,
        num_matching_decks,
        deck_state.current_decks.len(),
        pattern.iter().map(|p| p.to_string()).join(""),
        pattern_length
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn debug_decks(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
            .deck_state
            .current_decks
            .iter()
            .map(|vpol| vpol.iter().map(|pol| format!("{}", pol)).join(""))
            .join("\n")
    ))
}
