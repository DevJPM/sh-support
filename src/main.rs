use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str::Utf8Error
};

use contracts::{debug_ensures, debug_invariant};
use itertools::Itertools;
use repl_rs::{Command, Convert, Parameter, Repl, Value};
use std::{fmt, fs, io, result::Result, str};

//fn approx_one(value : f64) -> bool { (value - 1.0).abs() <= 1e-6 }

#[derive(Default, Debug)]
struct Context {
    num_cards : usize,
    current_decks : Vec<Vec<Policy>>,
    table_size : usize,
    num_regular_fascists : usize,
    available_information : Vec<Information>,
    current_roles : Vec<BTreeMap<PlayerID, SecretRole>>,
    player_info : BTreeMap<PlayerID, PlayerInfo>
}

impl Context {
    fn invariant(&self) -> bool {
        self.current_decks.iter().all(|d| d.len() == self.num_cards)
            && self.current_decks.iter().all_unique()
            && self.num_regular_fascists <= self.table_size
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
            })
    }
}

#[derive(Debug)]
enum Error {
    BadPlayerID(PlayerID),
    ParsePolicyError(String),
    ParseRoleError(String),
    FileSystemError(io::Error),
    TooLongPatternError { have : usize, requested : usize },
    ReplError(repl_rs::Error)
}

impl From<repl_rs::Error> for Error {
    fn from(error : repl_rs::Error) -> Self { Error::ReplError(error) }
}

impl From<Utf8Error> for Error {
    fn from(error : Utf8Error) -> Self { Error::ParsePolicyError(error.to_string()) }
}

impl From<io::Error> for Error {
    fn from(error : io::Error) -> Self { Error::FileSystemError(error) }
}

impl fmt::Display for Error {
    fn fmt(&self, f : &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Error::TooLongPatternError { have, requested } => write!(
                f,
                "Requested a pattern of length {} but only had {} cards in the decks.",
                requested, have
            ),
            Error::ParsePolicyError(found) => write!(
                f,
                "Failed to parse single-letter policy name, found {} instead.",
                found
            ),
            Error::ParseRoleError(found) => write!(
                f,
                "Failed to parse role name name, found {} instead.",
                found
            ),
            Error::ReplError(error) => write!(f, "{}", error),
            Error::BadPlayerID(id) => write!(f, "Failed to recognize player {}.", id),
            Error::FileSystemError(fserror) => write!(f, "Filesystem error: {fserror}")
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Policy {
    Liberal,
    Fascist
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum SecretRole {
    Liberal,
    RegularFascist,
    Hitler
}

impl fmt::Display for SecretRole {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretRole::Liberal => write!(f, "Liberal"),
            SecretRole::RegularFascist => write!(f, "Fascist"),
            SecretRole::Hitler => write!(f, "Hitler")
        }
    }
}

impl str::FromStr for SecretRole {
    type Err = Error;

    fn from_str(s : &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "h" | "hitler" => Ok(SecretRole::Hitler),
            "f" | "fascist" => Ok(SecretRole::RegularFascist),
            "l" | "b" | "lib" | "blue" | "liberal" => Ok(SecretRole::Liberal),
            _ => Err(Error::ParseRoleError(s.to_owned()))
        }
    }
}

impl SecretRole {
    fn is_fascist(&self) -> bool { !matches!(self, SecretRole::Liberal) }
}

type PlayerID = usize;
type PlayerInfo = String;

#[derive(Copy, Clone, Debug)]
enum Information {
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

fn valid_role_assignments(
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

impl std::error::Error for Error {}

#[debug_invariant(context.invariant())]
fn generate(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    context.current_decks.clear();
    context.num_cards = 0;

    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    let deck_size = num_lib + num_fasc;

    context.num_cards = deck_size;
    context.current_decks = (0..deck_size)
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
        context.current_decks.len(),
        num_lib,
        num_fasc
    )))
}

#[debug_invariant(context.invariant())]
fn dist(args : HashMap<String, Value>, context : &mut Context) -> Result<Option<String>, Error> {
    let window_size : usize = args["window_size"].convert()?;

    if window_size > context.num_cards {
        return Err(Error::TooLongPatternError {
            have : context.num_cards,
            requested : window_size
        });
    }

    let histogram = compute_window_histogram(&context.current_decks, window_size);

    let deck_count = context.current_decks.len();

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

fn exit(_args : HashMap<String, Value>, _context : &mut Context) -> Result<Option<String>, Error> {
    std::process::exit(0);
}

#[debug_invariant(context.invariant())]
fn next(args : HashMap<String, Value>, context : &mut Context) -> Result<Option<String>, Error> {
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

    if pattern_length > context.num_cards {
        return Err(Error::TooLongPatternError {
            have : context.num_cards,
            requested : pattern_length
        });
    }

    let num_lib_in_pattern = pattern.iter().filter(|p| **p == Policy::Liberal).count();

    let num_matching_decks = context
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

    let probability = (num_matching_decks as f64) / (context.current_decks.len() as f64);

    Ok(Some(format!(
        "There is a {:.1}% ({}/{}) chance for the claim pattern {} to match the next {} cards.",
        probability * 100.0,
        num_matching_decks,
        context.current_decks.len(),
        pattern.iter().map(|p| p.to_string()).join(""),
        pattern_length
    )))
}

#[debug_invariant(context.invariant())]
fn debug_decks(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
            .current_decks
            .iter()
            .map(|vpol| vpol.iter().map(|pol| format!("{}", pol)).join(""))
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
fn roles(args : HashMap<String, Value>, context : &mut Context) -> Result<Option<String>, Error> {
    context.current_roles.clear();
    context.available_information.clear();

    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    let table_size = num_fasc + num_lib + 1;
    context.table_size = table_size;
    context.num_regular_fascists = num_fasc;
    context.player_info = (1..=table_size)
        .into_iter()
        .map(|pid| (pid, "".to_string()))
        .collect();

    context.current_roles = (0..num_fasc + num_lib)
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
        context.current_roles.len(),
        num_lib,
        num_fasc
    )))
}

#[debug_invariant(context.invariant())]
fn debug_roles(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
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
fn show_facts(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    Ok(Some(
        context
            .available_information
            .iter()
            .enumerate()
            .map(|(index, information)| format!("{}. {}", index + 1, information))
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
fn add_hard_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let factual_position : PlayerID = args["player_position"].convert()?;
    let factual_role : String = args["role"].convert()?;
    let factual_role : SecretRole = factual_role.parse()?;

    context
        .available_information
        .push(Information::HardFact(factual_position, factual_role));

    Ok(Some(format!(
        "Successfully added the information that player {} is {} to the fact database.",
        factual_position, factual_role
    )))
}

#[debug_invariant(context.invariant())]
fn add_conflict(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let president : PlayerID = args["president"].convert()?;
    let chancellor : PlayerID = args["chancellor"].convert()?;

    context
        .available_information
        .push(Information::PolicyConflict(president, chancellor));

    Ok(Some(format!(
        "Successfully added the conflict between {president} and {chancellor} to the fact \
         database."
    )))
}

#[debug_invariant(context.invariant())]
fn liberal_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let investigator : PlayerID = args["investigator"].convert()?;
    let investigatee : PlayerID = args["investigatee"].convert()?;

    context
        .available_information
        .push(Information::LiberalInvestigation {
            investigator,
            investigatee
        });

    Ok(Some(format!(
        "Successfully added the liberal investigation of {investigator} on {investigatee} to the \
         fact database."
    )))
}

#[debug_invariant(context.invariant())]
fn fascist_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let investigator : PlayerID = args["investigator"].convert()?;
    let investigatee : PlayerID = args["investigatee"].convert()?;

    context
        .available_information
        .push(Information::FascistInvestigation {
            investigator,
            investigatee
        });

    Ok(Some(format!(
        "Successfully added the fascist investigation of {investigator} on {investigatee} to the \
         fact database."
    )))
}

#[debug_invariant(context.invariant())]
fn confirm_not_hitler(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let player : PlayerID = args["player"].convert()?;

    context
        .available_information
        .push(Information::ConfirmedNotHitler(player));

    Ok(Some(format!(
        "Successfully added the confirmation that player {player} is not Hitler to the database."
    )))
}

#[debug_invariant(context.invariant())]
fn remove_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let factual_position : usize = args["fact_to_be_removed"].convert()?;

    context.available_information.remove(factual_position - 1);

    Ok(Some(format!(
        "Successfully removed the fact #{factual_position} from the database."
    )))
}

#[debug_invariant(context.invariant())]
fn debug_filtered_roles(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let filtered_assignments = filter_assigned_roles(args, context)?;

    Ok(Some(
        filtered_assignments
            .into_iter()
            .map(|vpol| {
                vpol.iter()
                    .map(|(pos, role)| format!("({}: {})", pos, role))
                    .join(", ")
            })
            .join("\n")
    ))
}

fn filter_assigned_roles(
    args : HashMap<String, Value>,
    context : &Context
) -> Result<Vec<&BTreeMap<usize, SecretRole>>, Error> {
    let allow_fascist_fascist_conflict : bool = args["allow_fascist_fascist_conflict"].convert()?;
    let allow_aggressive_hitler : bool = args["allow_aggressive_hitler"].convert()?;

    let filtered_assignments = context
        .current_roles
        .iter()
        .filter(|roles| {
            valid_role_assignments(
                roles,
                &context.available_information,
                !allow_aggressive_hitler,
                !allow_fascist_fascist_conflict
            )
            .unwrap_or(false)
        })
        .collect_vec();
    Ok(filtered_assignments)
}

#[debug_invariant(context.invariant())]
fn impossible_teams(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let num_fascists = context.num_regular_fascists + 1;

    let filtered_assignments = filter_assigned_roles(args, context)?;

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

    let all_potential_fascist_teams = (1..=context.table_size)
        .combinations(num_fascists)
        .filter(|faspos| !legal_fascist_positions.contains(faspos))
        .collect_vec();

    Ok(Some(
        all_potential_fascist_teams
            .into_iter()
            .map(|vfas| vfas.into_iter().map(|fpos| format!("{fpos}")).join(", "))
            .map(|s| format!("{s} can't ALL be fascists at the same time."))
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
fn filtered_histogramm(
    args : HashMap<String, Value>,
    context : &Context
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

#[debug_invariant(context.invariant())]
fn hitler_snipe(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let histogram = filtered_histogramm(args, context)?;

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
                    "{}. Player {pid}: {:.1}% ({hitler_count}/{total_count}) chance of being \
                     Hitler.",
                    index + 1,
                    (hitler_count as f64 / total_count as f64) * 100.0
                )
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
fn liberal_percent(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let histogram = filtered_histogramm(args, context)?;

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
                    "Player {pid}: {:.1}% ({lib_count}/{total_count}) chance of being a liberal.",
                    (lib_count as f64 / total_count as f64) * 100.0
                )
            })
            .join("\n")
    ))
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
                "{pid} [{}]",
                vinfo
                    .into_iter()
                    .map(|info| match info {
                        Information::ConfirmedNotHitler(_) => {
                            format!("label=\"\\N\\nConfirmed not Hitler.\"")
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
fn graph(args : HashMap<String, Value>, context : &mut Context) -> Result<Option<String>, Error> {
    let filename : String = args["filename"].convert()?;

    let file_content = generate_dot_report(&context.available_information, &context.player_info);

    fs::write(format!("{filename}.dot"), file_content)?;

    Ok(Some(format!(
        "Run \"dot -Tpng -o {filename}.png {filename}.dot\" to generate the graph."
    )))
}

fn main() -> Result<(), Error> {
    Ok(Repl::new(Context::default())
        .use_completion(true)
        .with_description("Tool to assist with computational secret hitler questions.")
        .with_version("0.1.0")
        .with_name("sh-tool")
        .add_command(
            Command::new("generate", generate)
                .with_parameter(Parameter::new("num_lib").set_required(true)?)?
                .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
                .with_help(
                    "Generate a deck of specified parameters and store it as the current context."
                )
        )
        .add_command(
            Command::new("debug_decks", debug_decks)
                .with_help("Prints out all decks in the current context.")
        )
        .add_command(Command::new("exit", exit).with_help("Exits this program."))
        .add_command(Command::new("quit", exit).with_help("Exits this program."))
        .add_command(
            Command::new("next", next)
                .with_parameter(Parameter::new("pattern").set_required(true)?)?
                .with_help(
                    "Computes the probability that the next few cards match the specified card \
                     counts (order is ignored). E.g. \"next BBR\" will match \"BBR,RBB,BRB,...\" "
                )
        )
        .add_command(
            Command::new("dist", dist)
                .with_parameter(Parameter::new("window_size").set_required(true)?)?
                .with_help(
                    "Computes the distribution of claim-like cards within the next window_size \
                     cards."
                )
        )
        .add_command(
            Command::new("roles", roles)
                .with_parameter(Parameter::new("num_fasc").set_required(true)?)?
                .with_parameter(Parameter::new("num_lib").set_required(true)?)?
                .with_help(
                    "Generates all legal role assignments for num_lib + num_fasc + 1 players."
                )
        )
        .add_command(
            Command::new("debug_roles", debug_roles)
                .with_help("Prints out all role assignments in the current context.")
        )
        .add_command(
            Command::new("hard_fact", add_hard_fact)
                .with_parameter(Parameter::new("player_position").set_required(true)?)?
                .with_parameter(Parameter::new("role").set_required(true)?)?
                .with_help("Adds a known hard fact about a player.")
        )
        .add_command(
            Command::new("debug_filtered_roles", debug_filtered_roles)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Shows all the possible role assignments filtered by the fact database."
                )
        )
        .add_command(
            Command::new("show_facts", show_facts)
                .with_help("Shows the entirety of the fact database with indices for removal.")
        )
        .add_command(
            Command::new("remove_fact", remove_fact)
                .with_parameter(Parameter::new("fact_to_be_removed").set_required(true)?)?
                .with_help("Removes the fact with the given index from the database.")
        )
        .add_command(
            Command::new("conflict", add_conflict)
                .with_parameter(Parameter::new("president").set_required(true)?)?
                .with_parameter(Parameter::new("chancellor").set_required(true)?)?
                .with_help(
                    "Adds a policy conflict between the president and the chancellor to the fact \
                     database."
                )
        )
        .add_command(
            Command::new("confirm_not_hitler", confirm_not_hitler)
                .with_parameter(Parameter::new("player").set_required(true)?)?
                .with_help("Confirms that the given player is not hitler.")
        )
        .add_command(
            Command::new("liberal_investigation", liberal_investigation)
                .with_parameter(Parameter::new("investigator").set_required(true)?)?
                .with_parameter(Parameter::new("investigatee").set_required(true)?)?
                .with_help(
                    "Adds an investigation with a liberal result by the investigator on the \
                     investigatee."
                )
        )
        .add_command(
            Command::new("fascist_investigation", fascist_investigation)
                .with_parameter(Parameter::new("investigator").set_required(true)?)?
                .with_parameter(Parameter::new("investigatee").set_required(true)?)?
                .with_help(
                    "Adds an investigation with a fascist result by the investigator on the \
                     investigatee."
                )
        )
        .add_command(
            Command::new("impossible_teams", impossible_teams)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Identifies teams of fascists that are impossible based on the current \
                     information."
                )
        )
        .add_command(
            Command::new("hitler_snipe", hitler_snipe)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Shows the probability of each player being hitler based on the current \
                     filtered information."
                )
        )
        .add_command(
            Command::new("liberal_percent", liberal_percent)
                .with_parameter(
                    Parameter::new("allow_fascist_fascist_conflict").set_required(true)?
                )?
                .with_parameter(Parameter::new("allow_aggressive_hitler").set_required(true)?)?
                .with_help(
                    "Shows the probability of each player being a liberal based on the current \
                     filtered information."
                )
        )
        .add_command(
            Command::new("graph", graph)
                .with_parameter(Parameter::new("filename").set_required(true)?)?
                .with_help("Generates the graph.")
        )
        .run()?)
}
