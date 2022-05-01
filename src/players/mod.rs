use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs
};

use contracts::debug_invariant;
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{
    error::Error, information::Information, secret_role::SecretRole, Context, PlayerID, PlayerInfo
};

mod filter_engine;
use filter_engine::*;

#[derive(Default, Debug)]
pub(crate) struct PlayerState {
    table_size : usize,
    num_regular_fascists : usize,
    available_information : Vec<Information>,
    current_roles : Vec<BTreeMap<PlayerID, SecretRole>>,
    player_info : BTreeMap<PlayerID, PlayerInfo>
}

impl PlayerState {
    pub(crate) fn invariant(&self) -> bool {
        self.num_regular_fascists <= self.table_size
            && self.player_info.len() == self.table_size
            && self.current_roles.iter().all(|ra| {
                ra.len() == self.table_size
                    && ra
                        .iter()
                        .filter(|(_pid, role)| **role == SecretRole::RegularFascist)
                        .count()
                        == self.num_regular_fascists
                    && ra
                        .iter()
                        .filter(|(_pid, role)| **role == SecretRole::Hitler)
                        .count()
                        == 1
                    && ra.iter().map(|(pid, _)| pid).collect_vec()
                        == self.player_info.iter().map(|(pid, _)| pid).collect_vec()
            })
            && filter_assigned_roles_invonvenient(self, true, true).is_ok()
    }
}

#[debug_invariant(context.invariant())]
pub(crate) fn roles(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    player_state.current_roles.clear();
    player_state.available_information.clear();

    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    let table_size = num_fasc + num_lib + 1;
    player_state.table_size = table_size;
    player_state.num_regular_fascists = num_fasc;
    player_state.player_info = (1..=table_size)
        .into_iter()
        .map(|pid| (pid, "".to_string()))
        .collect();

    player_state.current_roles = (0..num_fasc + num_lib)
        .into_iter()
        .combinations(num_fasc)
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
        .collect_vec();

    Ok(Some(format!(
        "Successfully generated {} role-assignments with {} liberal and {} regular fascist roles \
         each.",
        player_state.current_roles.len(),
        num_lib,
        num_fasc
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn debug_roles(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
            .player_state
            .current_roles
            .iter()
            .map(|vpol| {
                vpol.iter()
                    .map(|(pos, role)| format!("({}: {})", pos, role))
                    .join(", ")
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn show_facts(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
            .player_state
            .available_information
            .iter()
            .enumerate()
            .map(|(index, information)| format!("{}. {}", index + 1, information))
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn add_hard_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let factual_position : PlayerID = args["player_position"].convert()?;
    let factual_role : String = args["role"].convert()?;
    let factual_role : SecretRole = factual_role.parse()?;

    if !player_state.player_info.contains_key(&factual_position) {
        return Err(Error::BadPlayerID(factual_position));
    }

    player_state
        .available_information
        .push(Information::HardFact(factual_position, factual_role));

    Ok(Some(format!(
        "Successfully added the information that player {} is {} to the fact database.",
        format_name(factual_position, &player_state.player_info),
        factual_role
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn add_conflict(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let president : PlayerID = args["president"].convert()?;
    let chancellor : PlayerID = args["chancellor"].convert()?;

    if !player_state.player_info.contains_key(&president) {
        return Err(Error::BadPlayerID(president));
    }

    if !player_state.player_info.contains_key(&chancellor) {
        return Err(Error::BadPlayerID(chancellor));
    }

    player_state
        .available_information
        .push(Information::PolicyConflict(president, chancellor));

    Ok(Some(format!(
        "Successfully added the conflict between {} and {} to the fact database.",
        format_name(president, &player_state.player_info),
        format_name(chancellor, &player_state.player_info)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn liberal_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let investigator : PlayerID = args["investigator"].convert()?;
    let investigatee : PlayerID = args["investigatee"].convert()?;

    if !player_state.player_info.contains_key(&investigator) {
        return Err(Error::BadPlayerID(investigator));
    }

    if !player_state.player_info.contains_key(&investigatee) {
        return Err(Error::BadPlayerID(investigatee));
    }

    player_state
        .available_information
        .push(Information::LiberalInvestigation {
            investigator,
            investigatee
        });

    Ok(Some(format!(
        "Successfully added the liberal investigation of {} on {} to the fact database.",
        format_name(investigator, &player_state.player_info),
        format_name(investigatee, &player_state.player_info)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn fascist_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let investigator : PlayerID = args["investigator"].convert()?;
    let investigatee : PlayerID = args["investigatee"].convert()?;

    if !player_state.player_info.contains_key(&investigator) {
        return Err(Error::BadPlayerID(investigator));
    }

    if !player_state.player_info.contains_key(&investigatee) {
        return Err(Error::BadPlayerID(investigatee));
    }

    player_state
        .available_information
        .push(Information::FascistInvestigation {
            investigator,
            investigatee
        });

    Ok(Some(format!(
        "Successfully added the fascist investigation of {} on {} to the fact database.",
        format_name(investigator, &player_state.player_info),
        format_name(investigatee, &player_state.player_info)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn confirm_not_hitler(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let player : PlayerID = args["player"].convert()?;

    if !player_state.player_info.contains_key(&player) {
        return Err(Error::BadPlayerID(player));
    }

    player_state
        .available_information
        .push(Information::ConfirmedNotHitler(player));

    Ok(Some(format!(
        "Successfully added the confirmation that player {} is not Hitler to the database.",
        format_name(player, &player_state.player_info)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn remove_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let factual_position : usize = args["fact_to_be_removed"].convert()?;

    if factual_position >= context.player_state.available_information.len() {
        return Err(Error::BadFactIndex(factual_position));
    }

    context
        .player_state
        .available_information
        .remove(factual_position - 1);

    Ok(Some(format!(
        "Successfully removed the fact #{factual_position} from the database."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn debug_filtered_roles(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let filtered_assignments = filter_assigned_roles(args, player_state)?;

    Ok(Some(
        filtered_assignments
            .into_iter()
            .map(|vpol| {
                vpol.iter()
                    .map(|(pos, role)| {
                        format!(
                            "({}: {})",
                            format_name(*pos, &player_state.player_info),
                            role
                        )
                    })
                    .join(", ")
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn impossible_teams(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let num_fascists = player_state.num_regular_fascists + 1;

    let filtered_assignments = filter_assigned_roles(args, player_state)?;

    let legal_fascist_positions = filtered_assignments
        .into_iter()
        .map(|ra| {
            ra.iter()
                .filter(|(_pos, role)| role.is_fascist())
                .map(|(pos, _role)| *pos)
                .sorted()
                .collect_vec()
        })
        .collect::<HashSet<_>>();

    let all_potential_fascist_teams = (1..=player_state.table_size)
        .combinations(num_fascists)
        .filter(|faspos| !legal_fascist_positions.contains(faspos))
        .collect_vec();

    Ok(Some(
        all_potential_fascist_teams
            .into_iter()
            .map(|vfas| {
                vfas.into_iter()
                    .map(|fpos| format_name(fpos, &player_state.player_info))
                    .join(" and ")
            })
            .map(|s| format!("{s} can't ALL be fascists at the same time."))
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn hitler_snipe(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let histogram = filtered_histogramm(args, player_state)?;

    Ok(Some(
        histogram
            .iter()
            .map(|(pid, roles)| {
                (
                    pid,
                    (
                        *roles.get(&SecretRole::Hitler).unwrap_or(&0),
                        roles.iter().map(|(_role, count)| count).sum::<usize>()
                    )
                )
            })
            .sorted_by_key(|(_pid, (hitler_count, _total_count))| -(*hitler_count as isize))
            .enumerate()
            .map(|(index, (pid, (hitler_count, total_count)))| {
                format!(
                    "{}. Player {}: {:.1}% ({hitler_count}/{total_count}) chance of being Hitler.",
                    index + 1,
                    format_name(*pid, &player_state.player_info),
                    (hitler_count as f64 / total_count as f64) * 100.0
                )
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn liberal_percent(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let histogram = filtered_histogramm(args, player_state)?;

    Ok(Some(
        histogram
            .iter()
            .map(|(pid, roles)| {
                (
                    pid,
                    (
                        *roles.get(&SecretRole::Liberal).unwrap_or(&0),
                        roles.iter().map(|(_role, count)| count).sum::<usize>()
                    )
                )
            })
            .sorted_by_key(|(pid, (_lib_count, _total_count))| *pid)
            .map(|(pid, (lib_count, total_count))| {
                format!(
                    "Player {}: {:.1}% ({lib_count}/{total_count}) chance of being a liberal.",
                    format_name(*pid, &player_state.player_info),
                    (lib_count as f64 / total_count as f64) * 100.0
                )
            })
            .join("\n")
    ))
}

fn format_name(pid : usize, players : &BTreeMap<PlayerID, PlayerInfo>) -> String {
    if let Some(name) = players.get(&pid) {
        if name.is_empty() {
            format!("{pid}")
        }
        else {
            format!("{pid}. {}", name)
        }
    }
    else {
        format!("{pid}")
    }
}

fn generate_dot_report(
    information : &Vec<Information>,
    players : &BTreeMap<PlayerID, PlayerInfo>
) -> String {
    let mut node_attributes : BTreeMap<PlayerID, Vec<Information>> = BTreeMap::new();
    players.iter().for_each(|(key, _name)| {
        node_attributes.insert(*key, vec![]);
    });
    let mut statements = vec![];

    let display_name = |pid| format_name(pid, players);

    for info in information {
        match info {
            Information::ConfirmedNotHitler(pid) => {
                node_attributes.entry(*pid).or_default().push(*info)
            },
            Information::PolicyConflict(left, right) => {
                statements.push(format!("{left} -> {right} [dir=both,color=red]"))
            },
            Information::LiberalInvestigation {
                investigator,
                investigatee
            } => statements.push(format!("{investigator} -> {investigatee} [color=blue]")),
            Information::FascistInvestigation {
                investigator,
                investigatee
            } => statements.push(format!("{investigator} -> {investigatee} [color=red]")),
            Information::HardFact(pid, _) => node_attributes.entry(*pid).or_default().push(*info)
        }
    }

    node_attributes
        .into_iter()
        .map(|(pid, vinfo)| {
            format!(
                "{pid} [label=\"{}\",{}]",
                display_name(pid),
                vinfo
                    .into_iter()
                    .map(|info| match info {
                        Information::ConfirmedNotHitler(_) => {
                            format!("label=\"{}\\nConfirmed not Hitler.\"", display_name(pid))
                        },
                        Information::HardFact(_pid, role) =>
                            format!("color={}", if role.is_fascist() { "red" } else { "blue" }),
                        _ => unreachable!()
                    })
                    .join(",")
            )
        })
        .for_each(|s| statements.push(s));

    let statements = statements.into_iter().join(";");

    format!("digraph {{{statements}}}")
}

#[debug_invariant(context.invariant())]
pub(crate) fn graph(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    let filename : String = args["filename"].convert()?;

    let file_content = generate_dot_report(
        &player_state.available_information,
        &player_state.player_info
    );

    fs::write(format!("{filename}.dot"), file_content)?;

    Ok(Some(format!(
        "Run \"dot -Tpng -o {filename}.png {filename}.dot\" to generate the graph."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn name(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let position : usize = args["position"].convert()?;
    let name : String = args["display_name"].convert()?;

    *context
        .player_state
        .player_info
        .get_mut(&position)
        .ok_or(Error::BadPlayerID(position))? = name.clone();

    Ok(Some(format!(
        "Successfully registered the name {name} for player {position}."
    )))
}
