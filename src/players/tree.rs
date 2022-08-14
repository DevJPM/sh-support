use std::collections::BTreeSet;

use contracts::debug_ensures;
use itertools::Itertools;

use crate::{
    deck::{complex_card_counter, FilterResult},
    information::Information,
    secret_role::SecretRole
};

use super::{
    filter_engine::filtered_histogramm, generate_claim_pattern_from_blues, ElectionResult,
    ElectionResult::*, PlayerInfos, PlayerManager, PlayerState, ShuffleAnalysis
};

#[derive(Clone)]
struct TreeNode {
    relative_probability : FilterResult,
    absolute_probability : f64,
    original_claimed_blues : usize,
    relevant_election_result : ElectionResult,
    children : Vec<TreeNode>
}

impl TreeNode {
    fn probability_check_recursive(nodes : &Vec<Self>) -> bool {
        nodes
            .iter()
            .map(|n| n.relative_probability.num_matching)
            .sum::<usize>()
            == nodes
                .iter()
                .map(|n| n.relative_probability.num_checked)
                .max()
                .unwrap_or(0)
            && nodes
                .iter()
                .all(|n| Self::probability_check_recursive(&n.children))
    }

    #[allow(dead_code)]
    fn invariant(&self) -> bool { Self::probability_check_recursive(&self.children) }

    fn pres_guaranteed_fasc(&self) -> bool {
        !matches!(&self.relevant_election_result, Election(eg) if eg.president_claimed_blues == self.original_claimed_blues)
            && !matches!(&self.relevant_election_result, TopDeck(_, _))
    }

    fn guaranteed_fasc_chancellor(&self) -> bool {
        matches!(&self.relevant_election_result, Election(eg) if (eg.president_claimed_blues as i64 - eg.chancellor_claimed_blues as i64).abs() > 1)
    }
}

pub(super) fn generate_probability_forest(player_state : &PlayerState) -> String {
    let mut trees = vec![];

    for shuffle in player_state.shuffle_election_results().iter() {
        let all_trees = generate_tree(shuffle);
        let consistent_trees = filter_paths(all_trees, |nodes| {
            logically_consistent_path_filter(nodes, shuffle, &player_state)
        });
        let relative_annotated_trees =
            annotate_trees_relative(consistent_trees, shuffle, &player_state);
        let absolute_annotated_trees = annotate_trees_absolute(relative_annotated_trees);
        trees.push(draw_tree(
            absolute_annotated_trees,
            shuffle,
            &player_state.player_info
        ));
    }

    format!("digraph{{{}}}", trees.into_iter().join(" ; "))
}

fn annotate_trees_absolute(relative_annotated_trees : Vec<TreeNode>) -> Vec<TreeNode> {
    relative_annotated_trees
        .into_iter()
        .map(|tn| annotate_trees_absolute_recursive(tn, 1.0))
        .collect()
}

fn annotate_trees_absolute_recursive(mut node : TreeNode, parent_probability : f64) -> TreeNode {
    node.absolute_probability = parent_probability * node.relative_probability.probability();
    node.children = node
        .children
        .into_iter()
        .map(|tn| annotate_trees_absolute_recursive(tn, node.absolute_probability))
        .collect();
    node
}

fn logically_consistent_path_filter(
    nodes : &[TreeNode],
    _shuffle : &ShuffleAnalysis,
    player_state : &PlayerState
) -> bool {
    let confirmed_deduced_path_fasc = nodes
        .iter()
        .flat_map(|tn| {
            [
                match &tn.relevant_election_result {
                    TopDeck(_, _) => None,
                    Election(eg) => tn.pres_guaranteed_fasc().then_some(eg.president)
                },
                match &tn.relevant_election_result {
                    TopDeck(_, _) => None,
                    Election(eg) => tn.guaranteed_fasc_chancellor().then_some(eg.chancellor)
                }
            ]
            .into_iter()
            .flatten()
        })
        .map(|id| Information::AtLeastOneFascist(vec![id]))
        .collect_vec();
    let histograms_if_consistent =
        filtered_histogramm((true, true), player_state, &confirmed_deduced_path_fasc);
    histograms_if_consistent.is_ok()
}

#[debug_ensures(TreeNode::probability_check_recursive(&ret))]
fn annotate_trees_relative(
    trees : Vec<TreeNode>,
    shuffle : &ShuffleAnalysis,
    player_state : &PlayerState
) -> Vec<TreeNode> {
    let hard_confirmed_libs = filtered_histogramm((true, true), player_state, &[])
        .map(|histogram| {
            histogram
                .into_iter()
                .filter_map(|(pid, (roles, _total))| {
                    roles
                        .get(&SecretRole::Liberal)
                        .map(|fr| (fr.num_checked == fr.num_matching).then_some(pid))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or(BTreeSet::new());

    let mut roots : Vec<TreeNode>;
    let follow_on_path_sets : Vec<Vec<Option<BTreeSet<usize>>>>;
    (roots, follow_on_path_sets) = trees
        .into_iter()
        .filter_map(|t| {
            let mut parents = vec![];
            annotate_trees_relative_recursive(
                shuffle,
                player_state,
                &hard_confirmed_libs,
                &mut parents,
                t,
                0
            )
        })
        .unzip();

    let follow_on_card_constraints = fold_children_legal_draws(follow_on_path_sets).unwrap();

    for child in roots.iter_mut() {
        child.relative_probability = complex_card_counter(
            shuffle.initial_deck_liberal,
            shuffle.initial_deck_fascist,
            &shuffle.election_results,
            &[],
            &follow_on_card_constraints,
            &hard_confirmed_libs,
            &BTreeSet::new(),
            &child.relevant_election_result
        );
    }

    roots
}

fn fold_children_legal_draws(
    follow_on_path_sets : Vec<Vec<Option<BTreeSet<usize>>>>
) -> Option<Vec<Option<BTreeSet<usize>>>> {
    follow_on_path_sets.into_iter().reduce(|lvec, rvec| {
        lvec.into_iter()
            .zip(rvec.into_iter())
            .map(|(lo, ro)| {
                lo.zip(ro)
                    .map(|(lset, rset)| lset.union(&rset).copied().collect())
            })
            .collect()
    })
}

fn annotate_trees_relative_recursive(
    shuffle_analysis : &ShuffleAnalysis,
    player_state : &PlayerState,
    hard_confirmed_libs : &BTreeSet<usize>,
    parent_path_nodes : &mut Vec<TreeNode>,
    mut node : TreeNode,
    depth : usize
) -> Option<(TreeNode, Vec<Option<BTreeSet<usize>>>)> {
    let confirmed_deduced_path_fasc = parent_path_nodes
        .iter()
        .flat_map(|tn| {
            [
                match &tn.relevant_election_result {
                    TopDeck(_, _) => None,
                    Election(eg) => tn.pres_guaranteed_fasc().then_some(eg.president)
                },
                match &tn.relevant_election_result {
                    TopDeck(_, _) => None,
                    Election(eg) => tn.guaranteed_fasc_chancellor().then_some(eg.chancellor)
                }
            ]
            .into_iter()
            .flatten()
        })
        .map(|id| Information::AtLeastOneFascist(vec![id]))
        .collect_vec();
    let parent_path_confirmed_libs =
        filtered_histogramm((true, true), player_state, &confirmed_deduced_path_fasc)
            .map(|histogram| {
                histogram
                    .into_iter()
                    .filter_map(|(pid, (roles, _total))| {
                        roles
                            .get(&SecretRole::Liberal)
                            .map(|fr| (fr.num_checked == fr.num_matching).then_some(pid))
                            .flatten()
                    })
                    .collect()
            })
            .unwrap(); // we filtered inconsistent paths out beforehand

    // leaf
    if node.children.is_empty() {
        let mut out_vec = vec![None; depth + 1];
        let parent_path_ers = parent_path_nodes
            .iter()
            .cloned()
            .map(|tn| tn.relevant_election_result)
            .collect_vec();
        let relative_probability = complex_card_counter(
            shuffle_analysis.initial_deck_liberal,
            shuffle_analysis.initial_deck_fascist,
            &shuffle_analysis.election_results,
            &parent_path_ers,
            &out_vec,
            &hard_confirmed_libs,
            &parent_path_confirmed_libs,
            &node.relevant_election_result
        );

        node.relative_probability = relative_probability;

        let _ = out_vec[depth].insert({
            let mut val = BTreeSet::new();
            val.insert(node.relevant_election_result.seen_blues());
            val
        });

        let keep = node.relative_probability.num_matching > 0;
        keep.then_some((node, out_vec))
    }
    else {
        let mut children = std::mem::take(&mut node.children);
        parent_path_nodes.push(node);
        let follow_on_card_constraints : Vec<Vec<Option<BTreeSet<usize>>>>;
        (children, follow_on_card_constraints) = children
            .into_iter()
            .filter_map(|c| {
                annotate_trees_relative_recursive(
                    shuffle_analysis,
                    player_state,
                    &hard_confirmed_libs,
                    parent_path_nodes,
                    c,
                    depth + 1
                )
            })
            .unzip();
        node = parent_path_nodes.pop().unwrap();
        node.children = children;
        let mut follow_on_card_constraints = fold_children_legal_draws(follow_on_card_constraints)?;

        let parent_path_ers = parent_path_nodes
            .iter()
            .cloned()
            .map(|tn| tn.relevant_election_result)
            .chain(std::iter::once(node.relevant_election_result.clone()))
            .collect_vec();

        node.children = node
            .children
            .into_iter()
            .filter_map(|mut child| {
                child.relative_probability = complex_card_counter(
                    shuffle_analysis.initial_deck_liberal,
                    shuffle_analysis.initial_deck_fascist,
                    &shuffle_analysis.election_results,
                    &parent_path_ers,
                    &follow_on_card_constraints,
                    &hard_confirmed_libs,
                    &parent_path_confirmed_libs,
                    &child.relevant_election_result
                );
                (child.relative_probability.num_matching > 0).then_some(child)
            })
            .collect();

        let _ = follow_on_card_constraints[depth].insert({
            let mut val = BTreeSet::new();
            val.insert(node.relevant_election_result.seen_blues());
            val
        });

        Some((node, follow_on_card_constraints))
    }
}

fn filter_paths(
    tree : Vec<TreeNode>,
    filter_predicate : impl Fn(&[TreeNode]) -> bool
) -> Vec<TreeNode> {
    tree.into_iter()
        .filter_map(|t| {
            let mut parents = vec![];
            filter_paths_recursive(&mut parents, t, &filter_predicate)
        })
        .collect()
}

fn filter_paths_recursive(
    parents : &mut Vec<TreeNode>,
    mut node : TreeNode,
    filter_predicate : &impl Fn(&[TreeNode]) -> bool
) -> Option<TreeNode> {
    // leaf
    if node.children.is_empty() {
        parents.push(node);
        let keep = filter_predicate(parents);
        let node = parents.pop().unwrap();
        keep.then_some(node)
    }
    else {
        let children = std::mem::take(&mut node.children);
        parents.push(node);
        let children = children
            .into_iter()
            .filter_map(|c| filter_paths_recursive(parents, c, filter_predicate))
            .collect();
        let mut node = parents.pop().unwrap();
        node.children = children;
        (!node.children.is_empty()).then_some(node)
    }
}

fn draw_tree(
    tree : Vec<TreeNode>,
    election_results : &ShuffleAnalysis<'_>,
    player_info : &PlayerInfos
) -> String {
    let root_name = format!("{}", election_results.shuffle_index);

    tree.iter()
        .enumerate()
        .flat_map(|(cid, tn)| {
            draw_tree_recursive(&root_name, &format!("{root_name}{cid}"), tn, player_info)
        })
        .chain(std::iter::once(format!(
            "{root_name} [label=\"Shuffle #{}\"]",
            election_results.shuffle_index + 1
        )))
        .join(";")
}

fn draw_tree_recursive(
    parent_name : &str,
    my_name : &str,
    node : &TreeNode,
    player_info : &PlayerInfos
) -> Vec<String> {
    let node_name = match &node.relevant_election_result {
        TopDeck(p, _) => format!("Top-Deck: {p}"),
        Election(eg) => format!(
            "Assumed Draw: {}\\nPresident {}: {}\\nChancellor {}: {}",
            generate_claim_pattern_from_blues(eg.president_claimed_blues, 3),
            player_info.format_name(eg.president),
            generate_claim_pattern_from_blues(node.original_claimed_blues, 3),
            player_info.format_name(eg.chancellor),
            generate_claim_pattern_from_blues(eg.chancellor_claimed_blues, 2)
        )
    };

    let mut out_vec = vec![];

    out_vec.push(format!(
        "{parent_name} -> {my_name} [label=\"{:.1}%\"]",
        node.relative_probability.probability() * 100.0
    ));
    out_vec.push(format!(
        "{my_name} [label=\"{node_name}\\n{:.1}%\",color={},fontcolor={}]",
        node.absolute_probability * 100.0,
        if node.pres_guaranteed_fasc() {
            "red"
        }
        else {
            "blue"
        },
        if node.guaranteed_fasc_chancellor() {
            "red"
        }
        else {
            "black"
        }
    ));

    let mut processed_children = node
        .children
        .iter()
        .enumerate()
        .flat_map(|(cid, tn)| {
            draw_tree_recursive(my_name, &format!("{my_name}{cid}"), tn, player_info)
        })
        .collect();

    out_vec.append(&mut processed_children);

    out_vec
}

fn generate_tree(election_results : &ShuffleAnalysis<'_>) -> Vec<TreeNode> {
    recursively_generate_tree(election_results.election_results.iter())
}

fn recursively_generate_tree<'a>(
    mut er_iter : impl Iterator<Item = &'a &'a ElectionResult> + Clone
) -> Vec<TreeNode> {
    if let Some(er) = er_iter.next() {
        let passed_blues = er.passed_blues();

        match er {
            TopDeck(_, _) => {
                let mut out_node = TreeNode {
                    relative_probability : FilterResult::none(1),
                    absolute_probability : 0.0,
                    original_claimed_blues : passed_blues,
                    relevant_election_result : (*er).clone(),
                    children : vec![]
                };
                out_node.children = recursively_generate_tree(er_iter);
                vec![out_node]
            },
            Election(eg) => (0..3)
                .into_iter()
                .map(|x| x + passed_blues)
                .map(|nbc| {
                    let mut copy = eg.clone();
                    copy.president_claimed_blues = nbc;
                    copy
                })
                .map(|neg| {
                    let neg = Election(neg);
                    let mut out_node = TreeNode {
                        relative_probability : FilterResult::none(1),
                        absolute_probability : 0.0,
                        original_claimed_blues : eg.president_claimed_blues,
                        relevant_election_result : neg,
                        children : vec![]
                    };

                    out_node.children = recursively_generate_tree(er_iter.clone());
                    out_node
                })
                .collect()
        }
    }
    else {
        vec![]
    }
}
