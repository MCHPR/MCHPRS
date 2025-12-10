use std::collections::hash_map::Entry;

use super::Pass;
use crate::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::passes::AnalysisInfos;
use crate::{CompilerInput, CompilerOptions};
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_world::World;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashMap;
use tracing::trace;

pub struct PruneRedundantLinks;

impl<W: World> Pass<W> for PruneRedundantLinks {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        let mut num_edges_pruned = 0;
        let node_indices = graph.node_indices().collect::<Vec<_>>();
        for idx in node_indices {
            num_edges_pruned += match graph[idx].ty {
                NodeType::Comparator { mode, .. } => prune_comparator_inputs(graph, idx, mode),
                _ => 0,
            };
        }
        trace!("Removed {num_edges_pruned} edges.");
    }

    fn status_message(&self) -> &'static str {
        "Pruning redundant links"
    }
}

/// Whenever a node has links to both the default input and the side input of a comparator,
/// only one of those links actually has an effect on the comparator (dominating link).
/// The other link's effect is always cancelled out by the dominating link.
/// This function determines the dominating link and removes the other.
///
/// The case where a node connects to both the default input and side input of a comparator is most
/// commonly seen in XOR gates implemented by 2 comparators in subtract mode.
fn prune_comparator_inputs(
    graph: &mut CompileGraph,
    idx: NodeIdx,
    comparator_mode: ComparatorMode,
) -> usize {
    let mut input_distances: FxHashMap<(NodeIdx, LinkType), u8> = FxHashMap::default();
    for edge in graph.edges_directed(idx, Direction::Incoming) {
        match input_distances.entry((edge.source(), edge.weight().ty)) {
            Entry::Occupied(occupied) => {
                let cur_distance = occupied.into_mut();
                *cur_distance = std::cmp::min(*cur_distance, edge.weight().ss);
            }
            Entry::Vacant(vacant) => {
                vacant.insert(edge.weight().ss);
            }
        };
    }

    let mut edges_to_be_removed = Vec::new();
    for edge in graph.edges_directed(idx, Direction::Incoming) {
        let default_distance = *input_distances
            .get(&(edge.source(), LinkType::Default))
            .unwrap_or(&u8::MAX);
        let side_distance = *input_distances
            .get(&(edge.source(), LinkType::Side))
            .unwrap_or(&u8::MAX);
        let dominating_input =
            dominating_comparator_input(comparator_mode, default_distance, side_distance);
        if edge.weight().ty != dominating_input {
            edges_to_be_removed.push(edge.id());
        }
    }

    let num_edges_pruned = edges_to_be_removed.len();
    for edge in edges_to_be_removed {
        graph.remove_edge(edge);
    }
    num_edges_pruned
}

fn dominating_comparator_input(
    comparator_mode: ComparatorMode,
    default_distance: u8,
    side_distance: u8,
) -> LinkType {
    match comparator_mode {
        ComparatorMode::Compare => {
            if default_distance <= side_distance {
                LinkType::Default
            } else {
                LinkType::Side
            }
        }
        ComparatorMode::Subtract => {
            if side_distance <= default_distance {
                LinkType::Side
            } else {
                LinkType::Default
            }
        }
    }
}
