use std::collections::{BTreeMap, HashMap};

use contracts::debug_invariant;
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{
    deck::FilterResult,
    error::{Error, Result},
    information::Information,
    secret_role::SecretRole,
    PlayerID
};

use super::PlayerState;

fn no_aggressive_hitler_filter(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &Information
) -> Result<bool> {
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
) -> Result<bool> {
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
) -> Result<bool> {
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
        Information::HardFact(pid, role) => Ok(lp(pid)? == role),
        Information::AtLeastOneFascist(vsp) => Ok(vsp
            .iter()
            .map(lp)
            .collect::<Result<Vec<_>>>()?
            .iter()
            .any(|role| role.is_fascist()))
    }
}

pub(super) fn valid_role_assignments(
    roles : &BTreeMap<PlayerID, SecretRole>,
    information : &[Information],
    no_aggressive_hitler : bool,
    no_fascist_fascist_conflict : bool
) -> Result<bool> {
    information
        .iter()
        .map(|i| {
            Ok(universal_deducable_information(roles, i)?
                && (!no_aggressive_hitler || no_aggressive_hitler_filter(roles, i)?)
                && (!no_fascist_fascist_conflict || no_fascist_fascist_conflict_filter(roles, i)?))
        })
        .collect::<Result<Vec<_>>>()
        .map(|vb| vb.into_iter().all(|x| x))
}

pub(super) fn filter_assigned_roles_inconvenient(
    player_state : &PlayerState,
    allow_fascist_fascist_conflict : bool,
    allow_aggressive_hitler : bool,
    temporary_infomration : &[Information]
) -> Result<Vec<BTreeMap<usize, SecretRole>>> {
    let filtered_assignments = player_state
        .current_roles()
        .into_iter()
        .filter(|roles| {
            valid_role_assignments(
                roles,
                &player_state
                    .collect_information()
                    .into_iter()
                    .chain(temporary_infomration.iter().cloned())
                    .collect_vec(),
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

pub(super) fn parse_filter_args(args : HashMap<String, Value>) -> Result<(bool, bool)> {
    let allow_fascist_fascist_conflict : bool = args["allow_fascist_fascist_conflict"].convert()?;
    let allow_aggressive_hitler : bool = args["allow_aggressive_hitler"].convert()?;

    Ok((allow_fascist_fascist_conflict, allow_aggressive_hitler))
}

pub(super) fn filter_assigned_roles(
    (allow_fascist_fascist_conflict, allow_aggressive_hitler) : (bool, bool),
    player_state : &PlayerState,
    temporary_infomration : &[Information]
) -> Result<Vec<BTreeMap<usize, SecretRole>>> {
    filter_assigned_roles_inconvenient(
        player_state,
        allow_fascist_fascist_conflict,
        allow_aggressive_hitler,
        temporary_infomration
    )
}

#[debug_invariant(player_state.invariant())]
pub(super) fn filtered_histogramm(
    (allow_fascist_fascist_conflict, allow_aggressive_hitler) : (bool, bool),
    player_state : &PlayerState,
    temporary_infomration : &[Information]
) -> Result<BTreeMap<PlayerID, (HashMap<SecretRole, FilterResult>, usize)>> {
    let filtered_assignments = filter_assigned_roles(
        (allow_fascist_fascist_conflict, allow_aggressive_hitler),
        player_state,
        temporary_infomration
    )?;

    Ok(filtered_assignments
        .into_iter()
        .flat_map(|ra| ra.into_iter())
        .sorted_by_key(|(pid, _role)| *pid)
        .group_by(|(pid, _role)| *pid)
        .into_iter()
        .map(|(pid, group)| {
            let counted = group
                .into_iter()
                .map(|(_pid, role)| role)
                .sorted()
                .group_by(|r| *r)
                .into_iter()
                .map(|(role, group)| {
                    (
                        role,
                        FilterResult {
                            num_matching : group.count(),
                            num_checked : 0
                        }
                    )
                })
                .collect::<HashMap<_, _>>();
            let total = counted.iter().map(|(_pid, count)| count.num_matching).sum();
            (
                pid,
                (
                    counted
                        .into_iter()
                        .map(|(pid, count)| {
                            (
                                pid,
                                FilterResult {
                                    num_matching : count.num_matching,
                                    num_checked : total
                                }
                            )
                        })
                        .collect(),
                    total
                )
            )
        })
        .collect())
}
