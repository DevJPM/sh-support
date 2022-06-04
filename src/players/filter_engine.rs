use std::collections::{BTreeMap, HashMap};

use contracts::debug_invariant;
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{error::Error, information::Information, secret_role::SecretRole, PlayerID};

use super::PlayerState;

fn no_aggressive_hitler_filter(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &Information
) -> Result<bool, Error> {
    let lp = |p| roles.get(p).ok_or(Error::BadPlayerID(*p));

    match information {
        Information::PolicyConflict(l, r) => {
            Ok(lp(l)? != &SecretRole::Hitler && lp(r)? != &SecretRole::Hitler)
        },
        Information::FascistInvestigation { investigator, .. } => {
            Ok(lp(investigator)? != &SecretRole::Hitler)
        },
        _ => Ok(true)
    }
}

fn no_fascist_fascist_conflict_filter(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &Information
) -> Result<bool, Error> {
    let lp = |p| roles.get(p).ok_or(Error::BadPlayerID(*p));

    match information {
        Information::PolicyConflict(l, r) => Ok(lp(l)?.is_fascist() != lp(r)?.is_fascist()),
        Information::FascistInvestigation {
            investigator,
            investigatee
        } => Ok(lp(investigator)?.is_fascist() != lp(investigatee)?.is_fascist()),
        _ => Ok(true)
    }
}

fn universal_deducable_information(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &Information
) -> Result<bool, Error> {
    let lp = |p| roles.get(p).ok_or(Error::BadPlayerID(*p));

    match information {
        Information::ConfirmedNotHitler(p) => Ok(*lp(p)? != SecretRole::Hitler),
        Information::PolicyConflict(l, r) => Ok(lp(l)?.is_fascist() || lp(r)?.is_fascist()),
        Information::LiberalInvestigation {
            investigator,
            investigatee
        } => Ok(lp(investigatee)? == &SecretRole::Liberal
            || (lp(investigator)?.is_fascist() && lp(investigatee)?.is_fascist())),
        Information::FascistInvestigation {
            investigator,
            investigatee
        } => Ok(lp(investigator)?.is_fascist() || lp(investigatee)?.is_fascist()),
        Information::HardFact(pid, role) => Ok(lp(pid)? == role)
    }
}

pub(super) fn valid_role_assignments(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &[Information],
    no_aggressive_hitler : bool,
    no_fascist_fascist_conflict : bool
) -> Result<bool, Error> {
    information
        .iter()
        .map(|i| {
            Ok(universal_deducable_information(roles, i)?
                && (!no_aggressive_hitler || no_aggressive_hitler_filter(roles, i)?)
                && (!no_fascist_fascist_conflict || no_fascist_fascist_conflict_filter(roles, i)?))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|vb| vb.into_iter().all(|x| x))
}

pub(super) fn filter_assigned_roles_inconvenient(
    player_state : &PlayerState,
    allow_fascist_fascist_conflict : bool,
    allow_aggressive_hitler : bool
) -> Result<Vec<&BTreeMap<usize, SecretRole>>, Error> {
    let filtered_assignments = player_state
        .current_roles
        .iter()
        .filter(|roles| {
            valid_role_assignments(
                roles,
                &player_state.available_information,
                !allow_aggressive_hitler,
                !allow_fascist_fascist_conflict
            )
            .unwrap_or(false)
        })
        .collect_vec();
    if filtered_assignments.is_empty() {
        Err(Error::LogicalInconsistency)
    }
    else {
        Ok(filtered_assignments)
    }
}

pub(super) fn filter_assigned_roles(
    args : HashMap<String, Value>,
    player_state : &PlayerState
) -> Result<Vec<&BTreeMap<usize, SecretRole>>, Error> {
    let allow_fascist_fascist_conflict : bool = args["allow_fascist_fascist_conflict"].convert()?;
    let allow_aggressive_hitler : bool = args["allow_aggressive_hitler"].convert()?;

    filter_assigned_roles_inconvenient(
        player_state,
        allow_fascist_fascist_conflict,
        allow_aggressive_hitler
    )
}

#[debug_invariant(context.invariant())]
pub(super) fn filtered_histogramm(
    args : HashMap<String, Value>,
    context : &PlayerState
) -> Result<BTreeMap<PlayerID, HashMap<SecretRole, usize>>, Error> {
    let filtered_assignments = filter_assigned_roles(args, context)?;

    Ok(filtered_assignments
        .into_iter()
        .flat_map(|ra| ra.iter())
        .map(|(pid, role)| (*pid, *role))
        .sorted_by_key(|(pid, _role)| *pid)
        .group_by(|(pid, _role)| *pid)
        .into_iter()
        .map(|(pid, group)| {
            (
                pid,
                group
                    .into_iter()
                    .map(|(_pid, role)| role)
                    .sorted()
                    .group_by(|r| *r)
                    .into_iter()
                    .map(|(role, group)| (role, group.count()))
                    .collect()
            )
        })
        .collect())
}
