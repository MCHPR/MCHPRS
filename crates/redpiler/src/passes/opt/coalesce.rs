//! # [`Coalesce`]
//!
//! Merges nodes that are indistinguishable at runtime: same type, same initial state and the same
//! multiset of input links. The merged node takes over all outgoing links and block positions.
//! Constants have no inputs, so all constants with the same strength merge into one.

use crate::compile_graph::{
    CompileGraph, CompileLink, Direction, EdgeRef, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use rustc_hash::{FxHashMap, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
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
        Coalescer::new(graph.node_bound()).run(graph);
    }

    fn status_message(&self) -> &'static str {
        "Combining duplicate logic"
    }

    fn driver_key(&self) -> &'static str {
        "coalesce"
    }
}

type InputLink = (NodeIdx, LinkType, u8);

#[derive(PartialEq, Eq, Hash)]
struct NodeSignature<'a> {
    ty: &'a NodeType,
    state: &'a NodeState,
    inputs: &'a [InputLink],
}

struct Coalescer {
    buckets: FxHashMap<u64, SmallVec<[NodeIdx; 1]>>,
    queued: Vec<bool>,
    inputs: Vec<InputLink>,
    candidate_inputs: Vec<InputLink>,
    moved_link_targets: Vec<NodeIdx>,
}

impl Coalescer {
    fn new(node_bound: usize) -> Self {
        Self {
            buckets: FxHashMap::with_capacity_and_hasher(node_bound, Default::default()),
            queued: vec![false; node_bound],
            inputs: Vec::new(),
            candidate_inputs: Vec::new(),
            moved_link_targets: Vec::new(),
        }
    }

    fn run(mut self, graph: &mut CompileGraph) {
        let mut worklist: Vec<NodeIdx> = graph.node_indices().collect();
        self.queued.fill(true);
        while !worklist.is_empty() {
            let (num_coalesced, changed) = self.run_iteration(graph, worklist);
            trace!("Iteration combined {} nodes", num_coalesced);
            worklist = changed;
        }
    }

    /// Returns the number of merged nodes and the nodes whose inputs changed.
    fn run_iteration(
        &mut self,
        graph: &mut CompileGraph,
        worklist: Vec<NodeIdx>,
    ) -> (usize, Vec<NodeIdx>) {
        let mut num_coalesced = 0;
        let mut changed = Vec::new();
        for idx in worklist {
            self.queued[idx.index()] = false;
            if !graph.contains_node(idx) {
                continue;
            }

            if !graph[idx].is_removable() {
                continue;
            }

            let signature = node_signature(graph, idx, &mut self.inputs);
            let bucket = self.buckets.entry(fx_hash(&signature)).or_default();
            let into = bucket.iter().copied().find(|&candidate| {
                candidate != idx
                    && graph.contains_node(candidate)
                    && signature == node_signature(graph, candidate, &mut self.candidate_inputs)
            });
            let Some(into) = into else {
                bucket.push(idx);
                continue;
            };

            coalesce(graph, idx, into, &mut self.moved_link_targets);
            num_coalesced += 1;
            for target in self.moved_link_targets.drain(..) {
                if !self.queued[target.index()] {
                    self.queued[target.index()] = true;
                    changed.push(target);
                }
            }
        }
        (num_coalesced, changed)
    }
}

fn node_signature<'a>(
    graph: &'a CompileGraph,
    idx: NodeIdx,
    inputs: &'a mut Vec<InputLink>,
) -> NodeSignature<'a> {
    let node = &graph[idx];
    inputs.clear();
    inputs.extend(graph.edges(idx, Direction::Incoming).map(|edge| {
        let link = edge.weight();
        (
            edge.source(),
            link.ty,
            significant_link_strength(graph, &node.ty, &edge),
        )
    }));
    inputs.sort_unstable();
    inputs.dedup_by(|weaker, strongest| weaker.0 == strongest.0 && weaker.1 == strongest.1);
    NodeSignature {
        ty: &node.ty,
        state: &node.state,
        inputs,
    }
}

fn fx_hash(signature: &NodeSignature<'_>) -> u64 {
    let mut hasher = FxHasher::default();
    signature.hash(&mut hasher);
    hasher.finish()
}

/// A binary source powers a binary reader through any link that can carry a signal at all,
/// so such a link is equivalent to a direct one.
fn significant_link_strength(
    graph: &CompileGraph,
    reader: &NodeType,
    edge: &EdgeRef<'_, CompileLink, u32>,
) -> u8 {
    let link = edge.weight();
    let source = &graph[edge.source()].ty;
    if link.ss < 15 && !reader.reads_signal_strength() && !source.outputs_signal_strength() {
        0
    } else {
        link.ss
    }
}

fn coalesce(
    graph: &mut CompileGraph,
    node: NodeIdx,
    into: NodeIdx,
    moved_link_targets: &mut Vec<NodeIdx>,
) {
    let mut outgoing = graph.neighbors(node, Direction::Outgoing).detach();
    while let Some((edge_idx, target)) = outgoing.next(graph) {
        let link = graph.remove_edge(edge_idx).unwrap();
        if target == into {
            continue;
        }
        let target = if target == node { into } else { target };
        graph.add_edge(into, target, link);
        moved_link_targets.push(target);
    }
    let mut node = graph.remove_node(node).unwrap();
    graph[into].block.append(&mut node.block);
}
