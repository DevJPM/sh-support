use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt, fs,
    ops::Deref,
    process::{Command, Stdio},
    rc::Rc
};

use arboard::{Clipboard, ImageData};
use contracts::debug_invariant;
use image::EncodableLayout;
use itertools::Itertools;
use repl_rs::{Convert, Value};

use crate::{
    deck::parse_pattern, error::Error, information::Information, policy::Policy,
    secret_role::SecretRole, Context, PlayerID
};

mod filter_engine;
use filter_engine::*;

type Callback = Rc<dyn Fn(&PlayerState, bool) -> Result<(), Error>>;

struct CallBackVec<T> {
    data : Vec<T>,
    callback : Callback
}

impl<T> Deref for CallBackVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target { &self.data }
}

impl<T> Default for CallBackVec<T> {
    fn default() -> Self {
        Self {
            data : Default::default(),
            callback : Rc::new(|_, _| Ok(()))
        }
    }
}

impl<T : fmt::Debug> fmt::Debug for CallBackVec<T> {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallBackVec")
            .field("data", &self.data)
            .finish()
    }
}

impl<T> CallBackVec<T> {
    #[must_use]
    fn push(&mut self, item : T) -> Callback {
        self.data.push(item);
        Rc::clone(&self.callback)
    }

    #[must_use]
    fn pop(&mut self) -> Option<Callback> { self.data.pop().map(|_| Rc::clone(&self.callback)) }

    #[must_use]
    fn remove(&mut self, index : usize) -> Callback {
        self.data.remove(index);
        Rc::clone(&self.callback)
    }
}

#[derive(Default, Debug)]
pub(crate) struct PlayerState {
    table_size : usize,
    num_regular_fascists : usize,
    available_information : CallBackVec<Information>,
    current_roles : Vec<BTreeMap<PlayerID, SecretRole>>,
    player_info : BTreeMap<PlayerID, PlayerInfo>,
    governments : CallBackVec<Government>
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
                    && valid_role_assignments(ra, &self.available_information, true, true).is_ok()
            })
            && self.current_roles.iter().all_unique()
            && self.player_info.iter().all(|(pid, pi)| pid == &pi.seat)
    }
}

fn parse_player_name(
    input : &str,
    registered_names : &BTreeMap<PlayerID, PlayerInfo>
) -> Result<PlayerID, Error> {
    if let Ok(numerical_indicator) = input.parse::<PlayerID>() {
        return Ok(numerical_indicator);
    }

    let input = input.to_lowercase();
    let registered_names = registered_names.clone();

    let sorted_by_score = registered_names
        .into_iter()
        .map(|(_id, pi)| pi)
        .map(|mut pi| {
            pi.name = pi.name.to_lowercase();
            pi
        })
        .filter(|pi| !pi.name.is_empty())
        .map(|pi| {
            let score = strsim::damerau_levenshtein(&input, &pi.name);
            (pi, score)
        })
        .sorted_by_key(|(_pi, score)| *score)
        .take(2)
        .collect_vec();

    if sorted_by_score.is_empty() {
        Err(Error::ParseNameError(input))
    }
    else if sorted_by_score.len() == 1 {
        let (pinfo, score) = &sorted_by_score[0];
        if *score >= 4 {
            Err(Error::ParseNameError(input))
        }
        else {
            Ok(pinfo.seat)
        }
    }
    else if sorted_by_score.len() == 2 {
        let (pinfo, score) = &sorted_by_score[0];
        let (_, backup_score) = &sorted_by_score[1];

        if backup_score.saturating_sub(2) < *score && *score != 0 {
            Err(Error::ParseNameError(input))
        }
        else {
            Ok(pinfo.seat)
        }
    }
    else {
        unreachable!()
    }
}

#[derive(Debug, Clone)]
struct Government {
    president : PlayerID,
    chancellor : PlayerID,
    president_claimed_blues : usize,
    chancellor_claimed_blues : usize,
    conflict : bool,
    policy_passed : Policy,
    killed_player : Option<PlayerID>
}

#[derive(Debug, Clone)]
struct PlayerInfo {
    seat : PlayerID,
    name : String
}

impl fmt::Display for PlayerInfo {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name.is_empty() {
            write!(f, "{}", self.seat)
        }
        else {
            write!(f, "{} {{{}}}", self.name, self.seat)
        }
    }
}

#[debug_invariant(context.invariant())]
pub(crate) fn roles(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let mut player_state = &mut context.player_state;
    *player_state = PlayerState::default();

    let num_lib : usize = args["num_lib"].convert()?;
    let num_fasc : usize = args["num_fasc"].convert()?;

    let table_size = num_fasc + num_lib + 1;
    player_state.table_size = table_size;
    player_state.num_regular_fascists = num_fasc;
    player_state.player_info = (1..=table_size)
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
    let factual_position : String = args["player_position"].convert()?;
    let factual_position = parse_player_name(&factual_position, &player_state.player_info)?;
    let factual_role : String = args["role"].convert()?;
    let factual_role : SecretRole = factual_role.parse()?;

    if !player_state.player_info.contains_key(&factual_position) {
        return Err(Error::BadPlayerID(factual_position));
    }

    player_state
        .available_information
        .push(Information::HardFact(factual_position, factual_role))(player_state, true)?;

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
    let president : String = args["president"].convert()?;
    let president = parse_player_name(&president, &player_state.player_info)?;
    let chancellor : String = args["chancellor"].convert()?;
    let chancellor = parse_player_name(&chancellor, &player_state.player_info)?;

    if !player_state.player_info.contains_key(&president) {
        return Err(Error::BadPlayerID(president));
    }

    if !player_state.player_info.contains_key(&chancellor) {
        return Err(Error::BadPlayerID(chancellor));
    }

    player_state
        .available_information
        .push(Information::PolicyConflict(president, chancellor))(player_state, true)?;

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
    let investigator : String = args["investigator"].convert()?;
    let investigator = parse_player_name(&investigator, &player_state.player_info)?;
    let investigatee : String = args["investigatee"].convert()?;
    let investigatee = parse_player_name(&investigatee, &player_state.player_info)?;

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
        })(player_state, true)?;

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
    let investigator : String = args["investigator"].convert()?;
    let investigator = parse_player_name(&investigator, &player_state.player_info)?;
    let investigatee : String = args["investigatee"].convert()?;
    let investigatee = parse_player_name(&investigatee, &player_state.player_info)?;

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
        })(player_state, true)?;

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
    let player : String = args["player"].convert()?;
    let player = parse_player_name(&player, &player_state.player_info)?;

    if !player_state.player_info.contains_key(&player) {
        return Err(Error::BadPlayerID(player));
    }

    player_state
        .available_information
        .push(Information::ConfirmedNotHitler(player))(player_state, true)?;

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

    if factual_position > context.player_state.available_information.len() || factual_position == 0
    {
        return Err(Error::BadFactIndex(factual_position));
    }

    context
        .player_state
        .available_information
        .remove(factual_position - 1)(&context.player_state, true)?;

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
    let player_state = &mut context.player_state;
    let num_fascists = player_state.num_regular_fascists + 1;

    let filtered_assignments = filter_assigned_roles(args, player_state)?;

    let legal_fascist_positions = filtered_assignments
        .into_iter()
        .map(|ra| {
            ra.iter()
                .filter(|(_pos, role)| role.is_fascist())
                .map(|(pos, _role)| *pos)
                .collect::<BTreeSet<_>>()
        })
        .collect_vec();

    let mut impossible_teams = vec![];

    for impossible_size in 1..=num_fascists {
        let mut local_impossible = (1..=player_state.table_size)
            .combinations(impossible_size)
            .map(|faspos| faspos.into_iter().collect::<BTreeSet<_>>())
            .filter(|faspos| {
                !impossible_teams
                    .iter()
                    .any(|discovered : &BTreeSet<_>| discovered.is_subset(faspos))
            })
            .filter(|faspos| {
                !legal_fascist_positions
                    .iter()
                    .any(|legal_fas| faspos.is_subset(legal_fas))
            })
            .collect_vec();
        impossible_teams.append(&mut local_impossible);
    }

    Ok(Some(
        impossible_teams
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
    players
        .get(&pid)
        .map(|p| format!("{p}"))
        .unwrap_or(format!("{pid}"))
}

fn generate_claim_pattern_from_blues(blues : usize) -> String {
    match blues {
        0 => "RRR".to_string(),
        1 => "RRB".to_string(),
        2 => "RBB".to_string(),
        3 => "BBB".to_string(),
        _ => unreachable!()
    }
}

fn generate_dot_report(
    information : &Vec<Information>,
    governments : &[Government],
    players : &BTreeMap<PlayerID, PlayerInfo>
) -> String {
    let mut node_attributes : BTreeMap<PlayerID, Vec<Information>> = BTreeMap::new();
    players.iter().for_each(|(key, _name)| {
        node_attributes.insert(*key, vec![]);
    });
    let mut statements = vec![];

    let display_name = |pid| format_name(pid, players);

    let mut handled_conflicts = BTreeSet::new();

    for (index, gov) in governments.iter().enumerate() {
        let mut chancellor_claim = generate_claim_pattern_from_blues(gov.chancellor_claimed_blues);
        chancellor_claim.remove(0);
        statements.push(format!(
            "{}->{} [label={},color={},dir={},taillabel={},headlabel={}]",
            gov.president,
            gov.chancellor,
            index + 1,
            if gov.policy_passed == Policy::Liberal {
                "blue"
            }
            else {
                "red"
            },
            if gov.conflict {
                handled_conflicts.insert((gov.president, gov.chancellor));
                "both"
            }
            else {
                "none"
            },
            generate_claim_pattern_from_blues(gov.president_claimed_blues),
            chancellor_claim
        ));
        if let Some(killed_player) = gov.killed_player {
            statements.push(format!(
                "{}->{} [label=killed, arrowhead=open]",
                gov.president, killed_player,
            ));
        }
    }

    for info in information {
        match info {
            Information::ConfirmedNotHitler(pid) => {
                node_attributes.entry(*pid).or_default().push(*info)
            },
            // only add this, if it was a manual conflict (i.e. if the two nodes don't already have
            // a gov-based conflict), e.g. to insert deck-peek based conflicts into the graph
            Information::PolicyConflict(left, right)
                if !handled_conflicts.contains(&(*left, *right))
                    && !handled_conflicts.contains(&(*right, *left)) =>
            {
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
            Information::HardFact(pid, _) => node_attributes.entry(*pid).or_default().push(*info),
            _ => {}
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

enum InvocationStrategy {
    Bash,
    Directly,
    None
}

#[debug_invariant(context.invariant())]
pub(crate) fn graph(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    //let mut player_state = &mut context.player_state;
    let filename : String = args["filename"].convert()?;
    let resp_filename = filename.clone();
    let auto_update : bool = args["auto"].convert()?;
    let executable : String = args["dot-invocation"].convert()?;
    let executable_l = executable.to_lowercase();

    let dotfile = format!("{filename}.dot");
    let imagefile = format!("{filename}.png");

    let options = vec![
        "-Tpng".to_string(),
        "-o".to_string(),
        imagefile.clone(),
        dotfile.clone(),
    ];

    let strategy = match executable_l.as_str() {
        "bash" => InvocationStrategy::Bash,
        "dot" => InvocationStrategy::Directly,
        "" => InvocationStrategy::None,
        _ => return Err(Error::BadExecutable(executable))
    };

    let baseline_command = executable_l;

    context.player_state.available_information.callback = Rc::new(move |ps, auto| {
        if !auto || auto_update {
            let file_content = generate_dot_report(
                ps.available_information.deref(),
                ps.governments.deref(),
                &ps.player_info
            );

            fs::write(&dotfile, file_content)?;

            let mut command = Command::new(&baseline_command);

            match strategy {
                InvocationStrategy::None => return Ok(()),
                InvocationStrategy::Bash => command
                    .arg("-c")
                    .arg(format! {"\"dot\" {}", options.iter().join(" ")}),
                InvocationStrategy::Directly => command.args(&options)
            };

            let dot_process = command
                .stdin(Stdio::null())
                .stderr(Stdio::piped())
                .stdout(Stdio::piped())
                .output()?;

            if !dot_process.stdout.is_empty() {
                return Err(Error::UnexpectedStdout(dot_process.stdout));
            }
            if !dot_process.stderr.is_empty() {
                return Err(Error::UnexpectedStderr(dot_process.stderr));
            }

            let image = image::io::Reader::open(&imagefile)?.decode()?;
            let image = image.as_rgba8().ok_or(Error::EncodingError)?;
            let mut clipboard = Clipboard::new()?;
            clipboard.set_image(ImageData {
                width : image.width() as usize,
                height : image.height() as usize,
                bytes : std::borrow::Cow::Borrowed(image.as_bytes())
            })?;

            fs::remove_file(&dotfile)?;
        }

        Ok(())
    });
    context.player_state.governments.callback =
        Rc::clone(&context.player_state.available_information.callback);
    context.player_state.governments.callback.clone()(&context.player_state, false)?;

    Ok(Some(format!(
        "Run \"dot -Tpng -o {resp_filename}.png {resp_filename}.dot\" to generate the graph."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn name(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let position : usize = args["position"].convert()?;
    let name : String = args["display_name"].convert()?;

    context
        .player_state
        .player_info
        .get_mut(&position)
        .ok_or(Error::BadPlayerID(position))?
        .name = name.clone();

    Ok(Some(format!(
        "Successfully registered the name {name} for player {position}."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn add_government(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let player_state = &mut context.player_state;
    let president : String = args["president"].convert()?;
    let president = parse_player_name(&president, &player_state.player_info)?;
    let chancellor : String = args["chancellor"].convert()?;
    let chancellor = parse_player_name(&chancellor, &player_state.player_info)?;
    let presidential_pattern : String = args["presidential_blues"].convert()?;
    let chancellor_pattern : String = args["chancellor_blues"].convert()?;
    let killed_player : usize = args["killed_player"].convert()?;
    let killed_player = if killed_player == 0 {
        None
    }
    else {
        Some(killed_player)
    };
    //let mut conflict : bool = args["conflict"].convert()?;

    let president_claimed_blues = parse_pattern(presidential_pattern, 3, 3)?.0;
    let chancellor_claimed_blues = parse_pattern(chancellor_pattern, 2, 2)?.0;

    let conflict = president_claimed_blues > 0 && chancellor_claimed_blues == 0;

    if !player_state.player_info.contains_key(&president) {
        return Err(Error::BadPlayerID(president));
    }
    if !player_state.player_info.contains_key(&chancellor) {
        return Err(Error::BadPlayerID(chancellor));
    }
    if let Some(killed_player) = killed_player {
        if !player_state.player_info.contains_key(&killed_player) {
            return Err(Error::BadPlayerID(killed_player));
        }
    }

    player_state.governments.push(Government {
        president,
        chancellor,
        president_claimed_blues,
        chancellor_claimed_blues,
        conflict,
        policy_passed : if (conflict && president_claimed_blues > 0) || president_claimed_blues == 0
        {
            Policy::Fascist
        }
        else {
            Policy::Liberal
        },
        killed_player
    })(player_state, true)?;

    if conflict {
        player_state
            .available_information
            .push(Information::PolicyConflict(president, chancellor))(player_state, true)?;
    }

    if let Some(killed_player) = killed_player {
        player_state
            .available_information
            .push(Information::ConfirmedNotHitler(killed_player))(player_state, true)?;
    }

    Ok(Some(format!(
        "Successfully added a government with president {} (claimed {president_claimed_blues} \
         blues) and chancellor {} (claimed {chancellor_claimed_blues} blues).",
        format_name(president, &player_state.player_info),
        format_name(chancellor, &player_state.player_info),
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn pop_government(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>, Error> {
    let last = context.player_state.governments.last().cloned();

    let callback = context.player_state.governments.pop();

    if let Some(removed) = last {
        if let Some(callback) = callback {
            callback(&context.player_state, true)?;
            Ok(Some(format!(
                "Successfully removed the last government with president {} and chancellor {}.",
                format_name(removed.president, &context.player_state.player_info),
                format_name(removed.chancellor, &context.player_state.player_info)
            )))
        }
        else {
            unreachable!()
        }
    }
    else {
        Ok(Some(
            "Successfully removed no government because none existed.".to_string()
        ))
    }
}
