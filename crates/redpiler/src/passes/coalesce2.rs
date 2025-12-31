//! # [`Coalesce2`]
//!
//! This pass combines duplicate logic, aka nodes with the same type state and inputs are merged into a single node.
//! For the best results run constant_fold2 first
//! This pass replaces coalesce.rs

use std::collections::HashSet;
use std::hash::Hash;

use super::Pass;
use crate::compile_graph::{CompileGraph, CompileLink, LinkType, NodeIdx, NodeState, NodeType};
use crate::passes::analysis::ss_range_analysis::SSRangeInfo;
use crate::passes::AnalysisInfos;
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction::{self, Incoming, Outgoing};
use rustc_hash::FxHashMap;

pub struct Coalesce2;

impl<W: World> Pass<W> for Coalesce2 {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let range_info: &mut SSRangeInfo = analysis_infos.get_analysis_mut().unwrap();
        run_pass(graph, range_info);
    }

    fn status_message(&self) -> &'static str {
        "Combining duplicate logic but better"
    }
}

type Input = (bool, NodeIdx, u8);

#[derive(PartialEq, Eq, Hash)]
struct Nod<'a> {
    inputs: &'a mut [Input],
    ty: NodeType,
    state: NodeState,
}

impl<'a> Nod<'a> {
    // Safety: The values in self.inputs should not be modified for the lifetime of the clone
    unsafe fn unsafe_clone(&self) -> Self {
        Self {
            inputs: std::slice::from_raw_parts_mut(
                self.inputs.as_ptr() as *mut Input,
                self.inputs.len(),
            ),
            ty: self.ty.clone(),
            state: self.state.clone(),
        }
    }
}

impl<'a> Default for Nod<'a> {
    fn default() -> Self {
        Self {
            inputs: Default::default(),
            ty: NodeType::Constant,
            state: Default::default(),
        }
    }
}

fn run_pass(graph: &mut CompileGraph, range_info: &mut SSRangeInfo) {
    let mut nod_inputs = vec![Input::default(); graph.edge_count()];
    let (mut nods, mut outputs, mut index_map, mut current) =
        to_nod_graph(graph, range_info, &mut nod_inputs);

    let mut next = Vec::<NodeIdx>::new();
    let mut nod_map: FxHashMap<Nod, NodeIdx> = FxHashMap::default();

    let mut dedup_output: HashSet<NodeIdx> = HashSet::new();
    let mut changes: Vec<(NodeIdx, NodeIdx)> = Vec::new();

    while current.len() > 0 {
        for old in current.iter().copied() {
            let mut idx = old;
            while idx != index_map[idx.index()] {
                idx = index_map[idx.index()];
            }
            index_map[old.index()] = idx;
            if old != idx {
                continue;
            }

            let nod = &mut nods[idx.index()];
            let inputs = std::mem::take(&mut nod.inputs);
            for i in inputs.iter_mut() {
                let mut ii = i.1;
                while ii != index_map[ii.index()] {
                    ii = index_map[ii.index()];
                }
                i.1 = ii;
            }
            inputs.sort();
            // Dedup
            let mut j = 0usize;
            {
                let mut last = (false, NodeIdx::end(), 0);
                for i in 0..inputs.len() {
                    let input = inputs[i];
                    if input == last {
                        continue;
                    }
                    inputs[j] = input;
                    last = input;
                    j += 1;
                }
            }
            nod.inputs = &mut inputs[..j];

            let Some(&same_node) = nod_map.get(&nod) else {
                // Safety: nod.inputs is not modified for the lifetime of this clone
                nod_map.insert(unsafe { nod.unsafe_clone() }, idx);
                continue;
            };

            changes.push((idx, same_node));

            let mut same_out = std::mem::take(&mut outputs[same_node.index()]);
            let this_out = &outputs[idx.index()];
            same_out.extend(this_out);

            for output in same_out.iter().copied() {
                let output = index_map[output.index()];

                if !dedup_output.insert(output) {
                    continue;
                }
                next.push(output);
            }

            outputs[same_node.index()] = same_out;

            let mut same_blocks = std::mem::take(&mut graph[same_node].block);
            let this_blocks = &graph[idx].block.as_slice();
            same_blocks.extend_from_slice(this_blocks);
            graph[same_node].block = same_blocks;
        }

        for (from, to) in changes.drain(..) {
            index_map[from.index()] = to;
        }

        dedup_output.clear();
        current.clear();
        nod_map.clear();
        std::mem::swap(&mut current, &mut next);
    }

    let (new_graph, new_range_info) = from_nod_graph(graph, range_info, &nods, &index_map);
    *graph = new_graph;
    *range_info = new_range_info;
}

fn to_nod_graph<'a>(
    graph: &petgraph::prelude::StableGraph<crate::compile_graph::CompileNode, CompileLink>,
    range_info: &SSRangeInfo,
    mut free_inputs: &'a mut [Input],
) -> (
    Box<[Nod<'a>]>,
    Box<[HashSet<NodeIdx>]>,
    Box<[NodeIdx]>,
    Vec<NodeIdx>,
) {
    // The initial capacity here should be enough to hold all inputs
    assert!(free_inputs.len() >= graph.edge_count());

    let mut nods: Vec<Nod> = Vec::with_capacity(graph.node_bound());

    let mut outputs: Vec<HashSet<NodeIdx>> = Vec::with_capacity(graph.node_bound());
    let mut index_map: Vec<NodeIdx> = Vec::with_capacity(graph.node_bound());
    let mut next: Vec<NodeIdx> = Vec::new();

    for i in 0..graph.node_bound() {
        let idx = NodeIdx::new(i);
        if !graph.contains_node(idx) {
            nods.push(Default::default());
            outputs.push(Default::default());
            index_map.push(NodeIdx::end());
            continue;
        }
        index_map.push(idx);
        outputs.push(graph.neighbors_directed(idx, Outgoing).collect());

        let node = &graph[idx];
        if !node.is_input {
            next.push(idx);
        }

        let is_bool = node.ty.is_bool();

        let mut len = 0;
        for edge in graph.edges_directed(idx, Incoming) {
            let source = edge.source();
            let weight = edge.weight();
            let ss_dist = weight.ss;
            let is_side = weight.ty == LinkType::Side;

            let possible_outputs = range_info.get_range(source).unwrap();
            let ss_distance = if is_bool {
                possible_outputs.normalize_bin_distance(ss_dist)
            } else {
                possible_outputs.normalize_hex_distance(ss_dist)
            };

            free_inputs[len] = (is_side, source, ss_distance);
            len += 1;
        }

        let (inputs, next_free) = free_inputs.split_at_mut(len);
        free_inputs = next_free;
        inputs.sort();

        let nod = Nod {
            inputs,
            ty: node.ty.clone(),
            state: node.state.clone(),
        };

        nods.push(nod);
    }
    (
        nods.into_boxed_slice(),
        outputs.into_boxed_slice(),
        index_map.into_boxed_slice(),
        next,
    )
}

fn from_nod_graph(
    graph: &CompileGraph,
    range_info: &SSRangeInfo,
    nods: &[Nod],
    index_map: &[NodeIdx],
) -> (CompileGraph, SSRangeInfo) {
    let mut old_to_new: Vec<NodeIdx> = vec![NodeIdx::end(); index_map.len()];

    let mut new_graph = CompileGraph::with_capacity(graph.node_count(), graph.edge_count());

    for i in graph.node_indices() {
        if index_map[i.index()] != i {
            continue;
        }

        old_to_new[i.index()] = new_graph.add_node(graph[i].clone());
    }

    for old_target in graph.node_indices() {
        if index_map[old_target.index()] != old_target {
            continue;
        }
        let new_target = old_to_new[old_target.index()];

        for (side, mut old_source, ss_dist) in nods[old_target.index()].inputs.iter().cloned() {
            while old_source != index_map[old_source.index()] {
                old_source = index_map[old_source.index()];
            }

            let new_source = old_to_new[old_source.index()];
            assert_ne!(new_source, NodeIdx::end());
            assert_ne!(new_target, NodeIdx::end());

            new_graph.add_edge(
                new_source,
                new_target,
                CompileLink {
                    ty: if side {
                        LinkType::Side
                    } else {
                        LinkType::Default
                    },
                    ss: ss_dist,
                },
            );
        }
    }

    let mut new_range_info = SSRangeInfo::with_reserved(new_graph.node_count());

    for (old, new) in old_to_new.iter().copied().enumerate() {
        if new == NodeIdx::end() {
            continue;
        }
        let old = NodeIdx::new(old);

        new_range_info.set_range(new, range_info.get_range(old).unwrap());
    }

    (new_graph, new_range_info)
}

pub fn coalesce(graph: &mut CompileGraph, node: NodeIdx, into: NodeIdx, extra_distance: u8) {
    if node == into {
        return;
    }

    let mut neighbors = graph.neighbors_directed(node, Direction::Outgoing).detach();
    while let Some((edge, dest)) = neighbors.next(graph) {
        let CompileLink { ty, ss } = graph[edge];
        let ss = ss + extra_distance;
        if ss < 15 {
            graph.add_edge(into, dest, CompileLink::new(ty, ss));
        }
    }

    let mut into_blocks = std::mem::take(&mut graph[into].block);
    let node_blocks = graph[node].block.as_slice();
    into_blocks.extend_from_slice(node_blocks);
    graph[into].block = into_blocks;

    graph.remove_node(node);
}
