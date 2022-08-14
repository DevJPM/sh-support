use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::{self},
    fs,
    ops::Deref,
    process::{Command, Stdio},
    rc::Rc
};

use arboard::{Clipboard, ImageData};
use contracts::debug_invariant;
use image::EncodableLayout;
use itertools::Itertools;
use repl_rs::{Convert, Value};
use serde::{Deserialize, Serialize};

use crate::{
    deck::{next_blues_count, parse_pattern, FilterResult},
    error::{Error, Result},
    information::Information,
    policy::Policy,
    secret_role::SecretRole,
    Context, PlayerID
};

mod filter_engine;
use filter_engine::*;
mod callback_vector;
use callback_vector::*;
pub mod game_configuration;
use game_configuration::*;
mod tree;
use tree::*;

/// CardContext always describes the situation before
/// the associated (set of) card(s) was drawn
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
pub(crate) struct CardContext {
    cards_left : usize,
    cards_discarded : usize,
    shuffle_index : usize
}

impl CardContext {
    fn atomic_draw(&self, draw_count : usize, discard_count : usize) -> Self {
        let mut out = *self;
        if self.cards_left.saturating_sub(draw_count) < 3 {
            out.cards_left += out.cards_discarded;
            out.cards_discarded = 0;
            out.shuffle_index += 1;
        }
        else {
            out.cards_discarded += discard_count;
            out.cards_left -= draw_count;
        }
        out
    }
}

pub(crate) type PlayerInfos = BTreeMap<PlayerID, PlayerInfo>;

pub(crate) trait PlayerManager<K> {
    fn format_name(&self, key : K) -> String;

    fn player_exists(&self, key : K) -> Result<()>;
}

#[derive(Debug)]
pub(crate) struct PlayerState {
    table_configuration : GameConfiguration,
    available_information : CallBackVec<Information>,
    player_info : PlayerInfos,
    governments : CallBackVec<ElectionResult>
}

impl PlayerState {
    pub(crate) fn current_roles(&self) -> Vec<BTreeMap<PlayerID, SecretRole>> {
        self.table_configuration.generate_assignments()
    }

    pub(crate) fn new(table_configuration : GameConfiguration) -> Self {
        let player_info = table_configuration.generate_default_info();
        Self {
            table_configuration,
            available_information : Default::default(),
            player_info,
            governments : Default::default()
        }
    }

    pub(crate) fn invariant(&self) -> bool {
        self.table_configuration.invariant()
            && self.player_info.len() == self.table_configuration.table_size
            && self.current_roles() == self.table_configuration.generate_assignments()
            && self.current_roles().iter().all(|ra| {
                ra.len() == self.table_configuration.table_size
                    && ra
                        .iter()
                        .filter(|(_pid, role)| **role == SecretRole::RegularFascist)
                        .count()
                        == self.table_configuration.num_regular_fascists
                    && ra
                        .iter()
                        .filter(|(_pid, role)| **role == SecretRole::Hitler)
                        .count()
                        == 1
                    && ra.iter().map(|(pid, _)| pid).collect_vec()
                        == self.player_info.iter().map(|(pid, _)| pid).collect_vec()
                    && valid_role_assignments(ra, &self.available_information, true, true).is_ok()
            })
            && self.current_roles().iter().all_unique()
            && self.player_info.iter().all(|(pid, pi)| pid == &pi.seat)
    }

    fn player_interactable(&self, player_id : PlayerID, player_info : &PlayerInfos) -> Result<()> {
        self.player_info.player_exists(player_id)?;
        validate_non_dead(player_id, &self.governments, player_info)?;

        Ok(())
    }

    fn count_policies_on_board(&self, policy : Policy) -> usize {
        self.governments
            .iter()
            .filter(|er| er.passed_policy() == policy)
            .count()
            + match policy {
                Policy::Liberal => self.table_configuration.initial_placed_liberal_policies,
                Policy::Fascist => self.table_configuration.initial_placed_fascist_policies
            }
    }

    fn is_eligible_chancellor(&self, player : PlayerID) -> bool {
        let players_alive = self.table_configuration.table_size
            - iter_elected(&self.governments)
                .filter(|g| matches!(g.presidential_action, Kill(_)))
                .count();
        match self.governments.last() {
            None => true,
            Some(TopDeck(_, _)) => true,
            Some(Election(gov)) => {
                gov.chancellor != player && (gov.president != player || players_alive <= 5)
            },
        }
    }

    fn is_eligible_president(&self, player : PlayerID) -> bool {
        let table_size = self.table_configuration.table_size;
        // we also can't "just" inspect the last government because people may have died
        // use an iota vector and a "cursor" to track the state and deaths, as well as
        // an option for special elections
        let mut current_president = 0;
        let mut next_president = 1; // needed for special elections
        let mut follow_on_president = 2;
        let mut dead_players = BTreeSet::new();

        let advance_mod_one = |next_president : &mut usize, follow_on_president : &mut usize| {
            *next_president = *follow_on_president;
            *follow_on_president += 1;
            if *follow_on_president > table_size {
                *follow_on_president = 1;
            }
        };

        let advance_one = |current_president : &mut usize,
                           next_president : &mut usize,
                           dead_players : &BTreeSet<usize>,
                           follow_on_president : &mut usize| {
            *current_president = *next_president;
            advance_mod_one(next_president, follow_on_president);
            while dead_players.contains(next_president) {
                advance_mod_one(next_president, follow_on_president);
            }
        };

        for er in self.governments.iter() {
            match er {
                TopDeck(_, _) => {
                    for _ in 0..3 {
                        advance_one(
                            &mut current_president,
                            &mut next_president,
                            &dead_players,
                            &mut follow_on_president
                        );
                    }
                },
                Election(gov) => {
                    while gov.president != current_president {
                        advance_one(
                            &mut current_president,
                            &mut next_president,
                            &dead_players,
                            &mut follow_on_president
                        );
                    }
                    match gov.presidential_action {
                        Kill(p) => {
                            dead_players.insert(p);
                        },
                        SpecialElection(np) => {
                            follow_on_president = next_president;
                            next_president = np;
                        },
                        _ => {}
                    }
                }
            }
        }

        for _ in 0..3 {
            advance_one(
                &mut current_president,
                &mut next_president,
                &dead_players,
                &mut follow_on_president
            );

            if current_president == player {
                return true;
            }
        }

        false
    }

    fn collect_information(&self) -> Vec<Information> {
        let peek_conflicts = iter_elected(&self.governments).tuple_windows().filter_map(
            |(first, second)| match first.presidential_action {
                TopDeckPeek(claim) => (second.president_claimed_blues
                    != claim.iter().filter(|x| x == &&Policy::Liberal).count())
                .then_some(Information::PolicyConflict(
                    first.president,
                    second.president
                )),
                PeekAndBurn(claim, false, _) => matches!(
                    (second.president_claimed_blues, claim),
                    (0, Policy::Liberal) | (3, Policy::Fascist)
                )
                .then_some(Information::PolicyConflict(
                    first.president,
                    second.president
                )),
                _ => None
            }
        );

        let immediate_conflicts = iter_elected(&self.governments).flat_map(|gov| {
            [
                gov.chancellor_confirmed_not_hitler
                    .then_some(Information::ConfirmedNotHitler(gov.chancellor)),
                gov.conflict
                    .then_some(Information::PolicyConflict(gov.president, gov.chancellor)),
                match gov.presidential_action {
                    NoAction => None,
                    Kill(dead_player) => Some(Information::ConfirmedNotHitler(dead_player)),
                    Investigation(investigatee, Policy::Fascist) => {
                        Some(Information::FascistInvestigation {
                            investigator : gov.president,
                            investigatee
                        })
                    },
                    Investigation(investigatee, Policy::Liberal) => {
                        Some(Information::LiberalInvestigation {
                            investigator : gov.president,
                            investigatee
                        })
                    },
                    RevealParty(investigator, Policy::Fascist) => {
                        Some(Information::FascistInvestigation {
                            investigator,
                            investigatee : gov.president
                        })
                    },
                    RevealParty(investigator, Policy::Liberal) => {
                        Some(Information::LiberalInvestigation {
                            investigator,
                            investigatee : gov.president
                        })
                    },
                    // peeks are handled by windowed pre-processing
                    _ => None
                }
            ]
            .into_iter()
            .flatten()
        });

        let shuffles = self.shuffle_election_results();

        let card_count_deductions = shuffles.iter().filter_map(|sa| {
            let seen_blues = sa.total_seen_blues();
            let governments = sa.election_results.iter().filter_map(|er| match er {
                TopDeck(_, _) => None,
                Election(eg) => Some(eg)
            });
            if seen_blues + sa.total_leftover < sa.initial_deck_liberal {
                Some(Information::AtLeastOneFascist(
                    governments
                        .filter_map(|eg| (eg.president_claimed_blues < 3).then_some(eg.president))
                        .collect()
                ))
            }
            else if seen_blues > sa.initial_deck_liberal {
                Some(Information::AtLeastOneFascist(
                    governments
                        .filter_map(|eg| (eg.president_claimed_blues > 0).then_some(eg.president))
                        .collect()
                ))
            }
            else {
                None
            }
        });

        immediate_conflicts
            .chain(peek_conflicts)
            .chain(card_count_deductions)
            .chain(self.available_information.iter().cloned())
            .collect()
    }

    fn shuffle_election_results(&self) -> Vec<ShuffleAnalysis<'_>> {
        let total_lib_cards = self.table_configuration.initial_placed_liberal_policies
            + self.table_configuration.initial_liberal_deck_policies;
        let total_fasc_cards = self.table_configuration.initial_placed_fascist_policies
            + self.table_configuration.initial_fascist_deck_policies;
        let total_cards = total_lib_cards + total_fasc_cards;

        self.governments
            .iter()
            .group_by(|er| match er {
                TopDeck(_, cc) => cc.shuffle_index,
                Election(gov) => gov.deck_context.shuffle_index
            })
            .into_iter()
            .scan(
                (
                    self.table_configuration.initial_placed_fascist_policies,
                    self.table_configuration.initial_placed_liberal_policies
                ),
                |(fpc, lpc), (idx, ver)| {
                    let election_results = ver.collect_vec();
                    let (total_drawn, total_discarded) = election_results
                        .iter()
                        .map(|er| er.cards_total_drawn_discarded())
                        .fold((0, 0), |(acc_l, acc_r), (cl, cr)| (acc_l + cl, acc_r + cr));
                    let (lfpc, llpc) = (*fpc, *lpc);
                    let blues_passed = election_results
                        .iter()
                        .filter(|er| er.passed_policy() == Policy::Liberal)
                        .count();
                    let reds_passed = election_results.len() - blues_passed;
                    *fpc += reds_passed;
                    *lpc += blues_passed;
                    Some(ShuffleAnalysis {
                        shuffle_index : idx,
                        election_results,
                        initial_deck_fascist : total_fasc_cards - lfpc,
                        initial_deck_liberal : total_lib_cards - llpc,
                        total_discarded,
                        total_leftover : total_cards - (lfpc + llpc) - total_drawn
                    })
                }
            )
            .collect()
    }

    fn build_next_card_context(&self) -> CardContext {
        if let Some(latest) = self.governments.last() {
            match latest {
                TopDeck(_, ctxt) => ctxt.atomic_draw(1, 0),
                Election(gov) => match gov.presidential_action {
                    PeekAndBurn(_, true, ctxt) => ctxt.atomic_draw(1, 1),
                    _ => gov.deck_context.atomic_draw(3, 2)
                }
            }
        }
        else {
            CardContext {
                cards_left : self.table_configuration.initial_fascist_deck_policies
                    + self.table_configuration.initial_liberal_deck_policies,
                cards_discarded : 0,
                shuffle_index : 0
            }
        }
    }
}

struct ShuffleAnalysis<'a> {
    shuffle_index : usize,
    election_results : Vec<&'a ElectionResult>,
    initial_deck_fascist : usize,
    initial_deck_liberal : usize,
    #[allow(dead_code)]
    total_discarded : usize,
    total_leftover : usize
}

impl ShuffleAnalysis<'_> {
    fn total_seen_blues(&self) -> usize {
        self.election_results.iter().map(|er| er.seen_blues()).sum()
    }
}

fn iter_elected(govs : &[ElectionResult]) -> impl Iterator<Item = &ElectedGovernment> {
    govs.iter().filter_map(|er| match er {
        TopDeck(_, _) => None,
        Election(gov) => Some(gov)
    })
}

impl PlayerManager<PlayerID> for PlayerInfos {
    fn format_name(&self, key : PlayerID) -> String {
        self.get(&key)
            .map(|p| format!("{p}"))
            .unwrap_or(format!("{key}"))
    }

    fn player_exists(&self, key : PlayerID) -> Result<()> {
        if !self.contains_key(&key) {
            Err(Error::BadPlayerID(key))
        }
        else {
            Ok(())
        }
    }
}

fn parse_player_name(
    input : &str,
    registered_names : &BTreeMap<PlayerID, PlayerInfo>
) -> Result<PlayerID> {
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

fn validate_non_dead(
    killed_player : usize,
    governments : &CallBackVec<ElectionResult>,
    player_info : &PlayerInfos
) -> Result<()> {
    if governments
        .iter()
        .any(|g| matches!(g, Election(g) if matches!(g.presidential_action, Kill(d) if d==killed_player )))
    {
        Err(Error::DeadPlayerID(killed_player,player_info.clone()))
    }
    else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(tag = "type", content = "content")]
pub(crate) enum PresidentialAction {
    NoAction,
    Kill(PlayerID),
    Investigation(PlayerID, Policy),
    RevealParty(PlayerID, Policy),
    TopDeckPeek([Policy; 3]),
    SpecialElection(PlayerID),
    /// true means discarded
    PeekAndBurn(Policy, bool, CardContext)
}

use PresidentialAction::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) enum ElectionResult {
    TopDeck(Policy, CardContext),
    Election(ElectedGovernment)
}

impl ElectionResult {
    pub(crate) fn cards_total_drawn_discarded(&self) -> (usize, usize) {
        match self {
            TopDeck(_, _) => (1, 0),
            Election(gov) => match gov.presidential_action {
                PeekAndBurn(_, true, _) => (4, 3),
                _ => (3, 2)
            }
        }
    }

    pub(crate) fn passed_policy(&self) -> Policy {
        match self {
            TopDeck(p, _) => *p,
            Election(gov) => gov.policy_passed
        }
    }

    pub(crate) fn seen_blues(&self) -> usize {
        match self {
            TopDeck(Policy::Liberal, _) => 1,
            Election(gov) => {
                gov.president_claimed_blues
                    + match gov.presidential_action {
                        PeekAndBurn(Policy::Liberal, true, _) => 1,
                        _ => 0
                    }
            },
            _ => 0
        }
    }

    pub(crate) fn passed_blues(&self) -> usize {
        if self.passed_policy() == Policy::Liberal {
            1
        }
        else {
            0
        }
    }

    //pub(crate) fn double
}

pub(crate) trait PlayerFormatable {
    fn format(&self, player_info : &PlayerInfos) -> String;
}

impl PlayerFormatable for ElectionResult {
    fn format(&self, player_info : &PlayerInfos) -> String {
        match self {
            TopDeck(card, _) => {
                format!("Enough elections failed resulting in a top deck of a {card} policy.")
            },
            Election(gov) => gov.format(player_info)
        }
    }
}

use ElectionResult::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct ElectedGovernment {
    pub president : PlayerID,
    pub chancellor : PlayerID,
    pub president_claimed_blues : usize,
    pub chancellor_claimed_blues : usize,
    pub conflict : bool,
    policy_passed : Policy,
    presidential_action : PresidentialAction,
    deck_context : CardContext,
    chancellor_confirmed_not_hitler : bool /* first president then chancellor votes
                                            * true = veto'ed
                                            *veto_result : Option<(bool, bool)> */
}

impl PlayerFormatable for ElectedGovernment {
    fn format(&self, player_info : &PlayerInfos) -> String {
        let presidential_action = match self.presidential_action {
            NoAction => "".to_string(),
            Kill(dead) => format!(
                "The president also decided to kill {}.",
                player_info.format_name(dead)
            ),
            Investigation(investigatee, result) => format!(
                "The president also investigated {} and claims to have found a {} party member.",
                player_info.format_name(investigatee),
                result
            ),
            RevealParty(investigator, result) => format!(
                "The president also showed their party membership to {} who claims to have seen \
                 {} party membership.",
                player_info.format_name(investigator),
                result
            ),
            TopDeckPeek(peek) => format!(
                "The president also looked at the top three cards of the deck and claims to have \
                 seen {}.",
                peek.iter().map(|s| format!("{s}")).join("")
            ),
            SpecialElection(electee) => format!(
                "The president also decided to appoint {} as the next president.",
                player_info.format_name(electee)
            ),
            PeekAndBurn(result, true, _) => format!(
                "The president also peeked at the top card of the deck and decided to discard the \
                 {result} policy."
            ),
            PeekAndBurn(result, false, _) => format!(
                "The president also looked at the top card of the deck and claims to have found a \
                 {result} policy without discarding it."
            )
        };
        format!(
            "President {} (claim: {}) and chancellor {} (claim: {}{}) passed a {} policy{} {}",
            player_info.format_name(self.president),
            generate_claim_pattern_from_blues(self.president_claimed_blues, 3),
            player_info.format_name(self.chancellor),
            generate_claim_pattern_from_blues(self.chancellor_claimed_blues, 2),
            if self.chancellor_confirmed_not_hitler {
                "; confirmed not Hitler now"
            }
            else {
                ""
            },
            self.policy_passed,
            if self.conflict {
                " which resulted in a conflict."
            }
            else {
                "."
            },
            presidential_action
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlayerInfo {
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
pub(crate) fn standard_game(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;

    let table_size : usize = args["player_count"].convert()?;
    let rebalanced : bool = args["rebalance"].convert()?;

    *player_state = PlayerState::new(GameConfiguration::new_standard(table_size, rebalanced)?);

    let num_reg_fasc = player_state.table_configuration.num_regular_fascists;

    Ok(Some(format!(
        "Successfully generated {} role-assignments ({}-player seat assignments) with {} liberal \
         and {} regular fascist roles each.",
        player_state.current_roles().len(),
        table_size,
        table_size - 1 - num_reg_fasc,
        num_reg_fasc
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn debug_roles(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    Ok(Some(
        context
            .player_state
            .current_roles()
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
) -> Result<Option<String>> {
    Ok(Some(format!(
        "Manually added facts with their removal index:\n{}",
        context
            .player_state
            .available_information
            .iter()
            .enumerate()
            .map(|(index, information)| {
                format!(
                    "{}. {}",
                    index + 1,
                    information.format(&context.player_state.player_info)
                )
            })
            .join("\n")
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn show_known_facts(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    Ok(Some(format!(
        "Manually added and deduced information:\n{}",
        context
            .player_state
            .collect_information()
            .iter()
            .enumerate()
            .map(|(index, information)| {
                format!(
                    "{}. {}",
                    index + 1,
                    information.format(&context.player_state.player_info)
                )
            })
            .join("\n")
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn show_governments(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    Ok(Some(
        context
            .player_state
            .governments
            .iter()
            .enumerate()
            .map(|(index, er)| {
                format!(
                    "{}. {}",
                    index + 1,
                    er.format(&context.player_state.player_info)
                )
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn add_hard_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let factual_position : String = args["player_position"].convert()?;
    let factual_position = parse_player_name(&factual_position, &player_state.player_info)?;
    let factual_role : String = args["role"].convert()?;
    let factual_role : SecretRole = factual_role.parse()?;

    player_state.player_info.player_exists(factual_position)?;

    player_state
        .available_information
        .push(Information::HardFact(factual_position, factual_role))(player_state, true)?;

    Ok(Some(format!(
        "Successfully added the information that player {} is {} to the fact database.",
        player_state.player_info.format_name(factual_position),
        factual_role
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn add_conflict(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let president : String = args["president"].convert()?;
    let president = parse_player_name(&president, &player_state.player_info)?;
    let chancellor : String = args["chancellor"].convert()?;
    let chancellor = parse_player_name(&chancellor, &player_state.player_info)?;

    player_state.player_info.player_exists(president)?;
    player_state.player_info.player_exists(president)?;

    player_state
        .available_information
        .push(Information::PolicyConflict(president, chancellor))(player_state, true)?;

    Ok(Some(format!(
        "Successfully added the conflict between {} and {} to the fact database.",
        player_state.player_info.format_name(president),
        player_state.player_info.format_name(chancellor)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn liberal_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let investigator : String = args["investigator"].convert()?;
    let investigator = parse_player_name(&investigator, &player_state.player_info)?;
    let investigatee : String = args["investigatee"].convert()?;
    let investigatee = parse_player_name(&investigatee, &player_state.player_info)?;

    player_state.player_info.player_exists(investigator)?;
    player_state.player_info.player_exists(investigatee)?;

    player_state
        .available_information
        .push(Information::LiberalInvestigation {
            investigator,
            investigatee
        })(player_state, true)?;

    Ok(Some(format!(
        "Successfully added the liberal investigation of {} on {} to the fact database.",
        player_state.player_info.format_name(investigator),
        player_state.player_info.format_name(investigatee)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn fascist_investigation(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let investigator : String = args["investigator"].convert()?;
    let investigator = parse_player_name(&investigator, &player_state.player_info)?;
    let investigatee : String = args["investigatee"].convert()?;
    let investigatee = parse_player_name(&investigatee, &player_state.player_info)?;

    player_state.player_info.player_exists(investigator)?;
    player_state.player_info.player_exists(investigatee)?;

    player_state
        .available_information
        .push(Information::FascistInvestigation {
            investigator,
            investigatee
        })(player_state, true)?;

    Ok(Some(format!(
        "Successfully added the fascist investigation of {} on {} to the fact database.",
        player_state.player_info.format_name(investigator),
        player_state.player_info.format_name(investigatee)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn confirm_not_hitler(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let player : String = args["player"].convert()?;
    let player = parse_player_name(&player, &player_state.player_info)?;

    player_state.player_info.player_exists(player)?;

    player_state
        .available_information
        .push(Information::ConfirmedNotHitler(player))(player_state, true)?;

    Ok(Some(format!(
        "Successfully added the confirmation that player {} is not Hitler to the database.",
        player_state.player_info.format_name(player)
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn remove_fact(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let factual_position : usize = args["fact_to_be_removed"].convert()?;

    if factual_position > context.player_state.available_information.len() || factual_position == 0
    {
        return Err(Error::BadFactIndex(factual_position));
    }

    context
        .player_state
        .available_information
        .remove(factual_position - 1)
        .ok_or(Error::BadFactIndex(factual_position))?(&context.player_state, true)?;

    Ok(Some(format!(
        "Successfully removed the fact #{factual_position} from the database."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn debug_filtered_roles(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let filtered_assignments = filter_assigned_roles(parse_filter_args(args)?, player_state, &[])?;

    Ok(Some(
        filtered_assignments
            .into_iter()
            .map(|vpol| {
                vpol.iter()
                    .map(|(pos, role)| {
                        format!("({}: {})", player_state.player_info.format_name(*pos), role)
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
) -> Result<Option<String>> {
    let player_state = &mut context.player_state;
    let num_fascists = player_state.table_configuration.num_regular_fascists + 1;

    let filtered_assignments = filter_assigned_roles(parse_filter_args(args)?, player_state, &[])?;

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
        let mut local_impossible = (1..=player_state.table_configuration.table_size)
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
                (
                    vfas.len(),
                    vfas.into_iter()
                        .map(|fpos| player_state.player_info.format_name(fpos))
                        .join(" and ")
                )
            })
            .map(|(pc, s)| {
                if pc != 1 {
                    format!("{s} can't ALL be fascists at the same time.")
                }
                else {
                    format!("{s} can't be a fascist.")
                }
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn hitler_snipe(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let histogram = filtered_histogramm(parse_filter_args(args)?, player_state, &[])?;

    Ok(Some(
        histogram
            .iter()
            .map(|(pid, (roles, total))| {
                (
                    pid,
                    roles
                        .get(&SecretRole::Hitler)
                        .copied()
                        .unwrap_or(FilterResult::none(*total))
                )
            })
            .sorted_by_key(|(_pid, fr)| -(fr.num_matching as isize))
            .enumerate()
            .map(|(index, (pid, fr))| {
                format!(
                    "{}. Player {}: {fr} chance of being Hitler.",
                    index + 1,
                    player_state.player_info.format_name(*pid),
                )
            })
            .join("\n")
    ))
}

#[debug_invariant(context.invariant())]
pub(crate) fn liberal_percent(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let histogram = filtered_histogramm(parse_filter_args(args)?, player_state, &[])?;

    Ok(Some(
        histogram
            .iter()
            .map(|(pid, (roles, total))| {
                (
                    pid,
                    roles
                        .get(&SecretRole::Liberal)
                        .copied()
                        .unwrap_or(FilterResult::none(*total))
                )
            })
            .map(|(pid, lib_count)| {
                format!(
                    "Player {}: {lib_count} chance of being a liberal.",
                    player_state.player_info.format_name(*pid)
                )
            })
            .join("\n")
    ))
}

fn generate_claim_pattern_from_blues(blues : usize, pattern_length : usize) -> String {
    let num_reds = pattern_length - blues;
    std::iter::repeat("R")
        .take(num_reds)
        .chain(std::iter::repeat("B").take(blues))
        .join("")
}

fn generate_dot_report(
    information : &Vec<Information>,
    governments : &[ElectionResult],
    players : &BTreeMap<PlayerID, PlayerInfo>
) -> String {
    let mut node_attributes : BTreeMap<PlayerID, Vec<Information>> = BTreeMap::new();
    players.iter().for_each(|(key, _name)| {
        node_attributes.insert(*key, vec![]);
    });
    let mut statements = vec![];

    let display_name = |pid| players.format_name(pid);

    let mut handled_conflicts = BTreeSet::new();

    for (index, gov) in governments.iter().enumerate() {
        match gov {
            Election(gov) => {
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
                    if gov.conflict
                        || information.iter().any(|info| matches!(
                            info,
                            Information::PolicyConflict(l, r) if (*l==gov.president && *r==gov.chancellor) || (*l==gov.chancellor && *r==gov.president)
                        ))
                    {
                        handled_conflicts.insert((gov.president, gov.chancellor));
                        "both"
                    }
                    else {
                        "none"
                    },
                    generate_claim_pattern_from_blues(gov.president_claimed_blues,3),
                    generate_claim_pattern_from_blues(gov.chancellor_claimed_blues, 2)
                ));
                if let Kill(killed_player) = gov.presidential_action {
                    statements.push(format!(
                        "{}->{} [label=killed, arrowhead=open]",
                        gov.president, killed_player,
                    ));
                }
            },
            TopDeck(_, _) => {}
        }
    }

    for info in information {
        match info {
            Information::ConfirmedNotHitler(pid) => {
                node_attributes.entry(*pid).or_default().push(info.clone())
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
            Information::HardFact(pid, _) => {
                node_attributes.entry(*pid).or_default().push(info.clone())
            },
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
) -> Result<Option<String>> {
    //let mut player_state = &mut context.player_state;
    let filename : String = args["filename"].convert()?;
    let resp_filename = filename.clone();
    let auto_update : bool = args["auto"].convert()?;
    let executable : String = args["dot-invocation"].convert()?;

    let dotfile = format!("{filename}.dot");
    let imagefile = format!("{filename}.png");

    let options = vec![
        "-Tpng".to_string(),
        "-o".to_string(),
        imagefile.clone(),
        dotfile.clone(),
    ];

    let (baseline_command, strategy) = executable_parser(executable)?;

    let closure : Callback = Rc::new(move |ps, auto| {
        if !auto || auto_update {
            let file_content = generate_dot_report(
                &ps.collect_information(),
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
            let image = image.as_rgba8().ok_or(Error::EncodingFailed)?;
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

    context
        .player_state
        .available_information
        .register_callback(CallbackKind::GovernmentOverviewGraph, Rc::clone(&closure));
    context
        .player_state
        .governments
        .register_callback(CallbackKind::GovernmentOverviewGraph, Rc::clone(&closure));
    context.player_state.governments.callback()(&context.player_state, false)?;

    Ok(Some(format!(
        "Run \"dot -Tpng -o {resp_filename}.png {resp_filename}.dot\" in a separate shell (e.g. \
         bash, cmd, powershell, ...) in the current working directory to generate the graph."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn name(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
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
) -> Result<Option<String>> {
    let player_state = &mut context.player_state;
    let president : String = args["president"].convert()?;
    let president = parse_player_name(&president, &player_state.player_info)?;
    let chancellor : String = args["chancellor"].convert()?;
    let chancellor = parse_player_name(&chancellor, &player_state.player_info)?;
    let presidential_pattern : String = args["presidential_blues"].convert()?;
    let chancellor_pattern : String = args["chancellor_blues"].convert()?;

    player_state.player_interactable(president, &player_state.player_info)?;
    player_state.player_interactable(chancellor, &player_state.player_info)?;

    if !player_state.is_eligible_president(president) {
        return Err(Error::NotEligiblePresident(
            president,
            context.player_state.player_info.clone()
        ));
    }

    if !player_state.is_eligible_chancellor(chancellor) || chancellor == president {
        return Err(Error::NotEligibleChancellor(
            chancellor,
            context.player_state.player_info.clone()
        ));
    }

    let president_claimed_blues = parse_pattern(presidential_pattern, 3, 3)?.0;
    let chancellor_claimed_blues = parse_pattern(chancellor_pattern, 2, 2)?.0;

    let immediate_conflict = president_claimed_blues > 0 && chancellor_claimed_blues == 0;

    let retrieve_player_opt_first = || -> Result<_> {
        let text_input : String = args["first_argument"].convert()?;
        let extraced_player = parse_player_name(&text_input, &player_state.player_info)?;
        player_state.player_interactable(extraced_player, &player_state.player_info)?;
        Ok(extraced_player)
    };

    let retrieve_policy_opt_second = || -> Result<_> {
        let text_input : String = args["second_argument"].convert()?;
        Ok(*parse_pattern(text_input, 1, 1)?.2.first().unwrap())
    };

    let retrieve_policy_opt_first = |count| -> Result<_> {
        let text_input : String = args["first_argument"].convert()?;
        Ok(parse_pattern(text_input, count, count)?.2)
    };

    let retrieve_boolean_opt_second = || -> Result<_> {
        let text_input : bool = args["first_argument"].convert()?;
        Ok(text_input)
    };

    let policy_passed =
        if (immediate_conflict && president_claimed_blues > 0) || president_claimed_blues == 0 {
            Policy::Fascist
        }
        else {
            Policy::Liberal
        };

    let prev_fas_policies = player_state.count_policies_on_board(Policy::Fascist);

    let deck_context = player_state.build_next_card_context();

    let presidential_action = if policy_passed == Policy::Fascist {
        if prev_fas_policies >= 5 {
            return Ok(Some("gg, fascists won.".to_string()));
        }

        match player_state.table_configuration.fascist_board_configuration[prev_fas_policies] {
            NoAction => NoAction,
            Kill(_) => retrieve_player_opt_first().map(Kill)?,
            Investigation(_, _) => {
                Investigation(retrieve_player_opt_first()?, retrieve_policy_opt_second()?)
            },
            RevealParty(_, _) => {
                RevealParty(retrieve_player_opt_first()?, retrieve_policy_opt_second()?)
            },
            TopDeckPeek(_) => TopDeckPeek(retrieve_policy_opt_first(3)?.try_into().unwrap()),
            SpecialElection(_) => retrieve_player_opt_first().map(SpecialElection)?,
            PeekAndBurn(_, _, _) => PeekAndBurn(
                *retrieve_policy_opt_first(1)?.first().unwrap(),
                retrieve_boolean_opt_second()?,
                deck_context.atomic_draw(3, 2)
            )
        }
    }
    else {
        NoAction
    };

    let government = ElectedGovernment {
        president,
        chancellor,
        president_claimed_blues,
        chancellor_claimed_blues,
        conflict : immediate_conflict,
        policy_passed,
        presidential_action,
        deck_context,
        chancellor_confirmed_not_hitler : prev_fas_policies
            >= player_state
                .table_configuration
                .hitler_zone_passed_fascist_policies
    };
    let government_text = government.format(&player_state.player_info);

    player_state
        .governments
        .push(ElectionResult::Election(government))(player_state, true)?;

    Ok(Some(format!(
        "Successfully added a government with the following events: {government_text}"
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn pop_government(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let govs = &mut context.player_state.governments;
    let last = govs.last().cloned();

    let callback = govs.remove(govs.len() - 1);

    if let Some(removed) = last {
        if let Some(callback) = callback {
            callback(&context.player_state, true)?;
            match removed {
                TopDeck(p, _) => Ok(Some(format!(
                    "Successfully removed the topdeck failed election which resulted in a {p} \
                     draw."
                ))),
                Election(gov) => Ok(Some(format!(
                    "Successfully removed the last government with the following events: {}",
                    gov.format(&context.player_state.player_info)
                )))
            }
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

#[debug_invariant(context.invariant())]
pub(crate) fn topdeck(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let drawn_policy : String = args["drawn_policy"].convert()?;
    let drawn_policy : Policy = drawn_policy.parse()?;

    context.player_state.governments.push(TopDeck(
        drawn_policy,
        context.player_state.build_next_card_context()
    ))(&context.player_state, true)?;

    Ok(Some(format!(
        "Successfully added a top-deck that resulted in a {drawn_policy} policy enactment."
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn total_draw_probability(
    _args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    Ok(Some(
        context
            .player_state
            .shuffle_election_results()
            .iter()
            .map(|sa| {
                let analysis = next_blues_count(
                    sa.initial_deck_liberal,
                    sa.initial_deck_fascist,
                    sa.total_leftover,
                    sa.initial_deck_liberal
                        .saturating_sub(sa.total_seen_blues()),
                    0,
                    0
                );

                format!(
                    "Assuming nobody lied, the shuffle #{} has a {analysis} chance of occuring.",
                    sa.shuffle_index + 1
                )
            })
            .join("\n")
    ))
}

// Can we use this probability information (perhaps reduced down for each
// layer?) to enrich the main government graph?

// TODO: can we / do we want to turn this into a DAG?
#[debug_invariant(context.invariant())]
pub(crate) fn probability_tree(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let filename : String = args["filename"].convert()?;
    let resp_filename = filename.clone();
    let auto_update : bool = args["auto"].convert()?;
    let executable : String = args["dot-invocation"].convert()?;

    let dotfile = format!("{filename}.dot");
    let imagefile = format!("{filename}.png");

    let options = vec![
        "-Tpng".to_string(),
        "-o".to_string(),
        imagefile.clone(),
        dotfile.clone(),
    ];

    let (baseline_command, strategy) = executable_parser(executable)?;

    let closure : Callback = Rc::new(move |ps, auto| {
        if !auto || auto_update {
            let file_content = generate_probability_forest(&ps);

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

            fs::remove_file(&dotfile)?;
        }

        Ok(())
    });

    context
        .player_state
        .available_information
        .register_callback(CallbackKind::ProbabilityTree, Rc::clone(&closure));
    context
        .player_state
        .governments
        .register_callback(CallbackKind::ProbabilityTree, Rc::clone(&closure));
    context.player_state.governments.callback()(&context.player_state, false)?;

    Ok(Some(format!(
        "Run \"dot -Tpng -o {resp_filename}.png {resp_filename}.dot\" in a separate shell (e.g. \
         bash, cmd, powershell, ...) in the current working directory to generate the graph."
    )))
}

fn executable_parser(executable : String) -> Result<(String, InvocationStrategy)> {
    let executable_l = executable.to_lowercase();
    let strategy = match executable_l.as_str() {
        "bash" => InvocationStrategy::Bash,
        "dot" => InvocationStrategy::Directly,
        "" => InvocationStrategy::None,
        _ => return Err(Error::BadExecutable(executable))
    };
    Ok((executable_l, strategy))
}

#[debug_invariant(context.invariant())]
pub(crate) fn create_game_config(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let filename : String = args["filename"].convert()?;

    let config = GameConfiguration::interactively_ask_for_configuration();

    fs::write(
        format!("{filename}.json"),
        serde_json::to_string_pretty(&config)?
    )?;

    context.player_state = PlayerState::new(config);

    Ok(Some(format!(
        "Successfully saved the configuration to {filename}.json. Also initialized the game with \
         {} possible role assignments.",
        context.player_state.current_roles().len()
    )))
}

#[debug_invariant(context.invariant())]
pub(crate) fn load_game_config(
    args : HashMap<String, Value>,
    context : &mut Context
) -> Result<Option<String>> {
    let mut player_state = &mut context.player_state;
    let filename : String = args["filename"].convert()?;

    *player_state = PlayerState::new(serde_json::from_slice(&fs::read(&filename)?)?);

    Ok(Some(format!(
        "Successfully loaded the {filename} configuration file. This resulted in a game with the \
         following characteristics: {}. {} possible role assignments for this table have been \
         loaded.",
        player_state.table_configuration,
        player_state.current_roles().len()
    )))
}
