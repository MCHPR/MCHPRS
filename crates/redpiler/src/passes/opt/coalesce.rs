//! # [`Coalesce`]
//!
//! Merges nodes with the same type, initial state and input links, except inputs and nodes with a
//! pending tick.

use crate::compile_graph::{
    CompileGraph, CompileLink, Direction, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::VecDeque;
use tracing::trace;

pub struct Coalesce;

impl<W: World> Pass<W> for Coalesce {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        // Signatures of nodes whose inputs changed name a removed node, so they never match again.
        let mut nodes_by_signature = FxHashMap::default();
        let mut queue: VecDeque<NodeIdx> = graph.node_indices().collect();
        let mut queued = vec![true; graph.node_bound()];
        let mut num_coalesced = 0;
        while let Some(node) = queue.pop_front() {
            queued[node.index()] = false;
            if !graph.contains_node(node) {
                continue;
            }
            if graph[node].is_input || graph[node].state.pending_tick {
                continue;
            }

            let signature = signature(graph, node);
            let is_shared_input = |idx| signature.inputs.iter().any(|&(source, ..)| source == idx);
            let into = match nodes_by_signature.get(&signature) {
                None => {
                    nodes_by_signature.insert(signature, node);
                    continue;
                }
                Some(&into) if is_shared_input(node) && is_shared_input(into) => continue,
                Some(&into) => into,
            };

            let into_links = if is_shared_input(into) {
                take_outgoing_links(graph, into)
            } else {
                Vec::new()
            };
            for (target, link) in take_outgoing_links(graph, node) {
                if target == into {
                    continue;
                }
                let target = if target == node { into } else { target };
                graph.add_edge(into, target, link);
                if !queued[target.index()] {
                    queued[target.index()] = true;
                    queue.push_back(target);
                }
            }
            for (target, link) in into_links {
                graph.add_edge(into, target, link);
            }
            let mut removed = graph.remove_node(node).unwrap();
            graph[into].block.append(&mut removed.block);
            num_coalesced += 1;
        }
        trace!("Coalesced {} nodes", num_coalesced);
    }

    fn status_message(&self) -> &'static str {
        "Combining duplicate logic"
    }

    fn driver_key(&self) -> &'static str {
        "coalesce"
    }
}

#[derive(PartialEq, Eq, Hash)]
struct Signature {
    ty: NodeType,
    state: NodeState,
    is_output: bool,
    inputs: SmallVec<[(NodeIdx, LinkType, u8); 4]>,
}

fn signature(graph: &CompileGraph, idx: NodeIdx) -> Signature {
    let node = &graph[idx];
    let mut inputs: SmallVec<_> = graph
        .edges(idx, Direction::Incoming)
        .map(|edge| {
            let link = edge.weight();
            let carries_binary_signal = link.ss < 15
                && node.ty.is_binary_reader()
                && graph[edge.source()].ty.is_binary_source();
            let weight = if carries_binary_signal { 0 } else { link.ss };
            (edge.source(), link.ty, weight)
        })
        .collect();
    inputs.sort_unstable();
    inputs.dedup_by_key(|&mut (source, ty, _)| (source, ty));
    Signature {
        ty: node.ty.clone(),
        state: node.state.clone(),
        is_output: node.is_output,
        inputs,
    }
}

fn take_outgoing_links(graph: &mut CompileGraph, node: NodeIdx) -> Vec<(NodeIdx, CompileLink)> {
    let mut links = Vec::new();
    let mut outgoing = graph.neighbors(node, Direction::Outgoing).detach();
    while let Some((edge_idx, target)) = outgoing.next(graph) {
        links.push((target, graph.remove_edge(edge_idx).unwrap()));
    }
    links
}
