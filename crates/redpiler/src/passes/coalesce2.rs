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

#[derive(PartialEq, Eq, Hash, Clone)]
struct Nod {
    inputs: Vec<(bool, NodeIdx, u8)>,
    ty: NodeType,
    state: NodeState,
}

impl Default for Nod {
    fn default() -> Self {
        Self {
            inputs: Default::default(),
            ty: NodeType::Constant,
            state: Default::default(),
        }
    }
}

fn run_pass(graph: &mut CompileGraph, range_info: &mut SSRangeInfo) {
    let (mut nods, mut outputs, mut index_map, mut current) = to_nod_graph(graph, range_info);

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
            for i in &mut nod.inputs {
                let mut ii = i.1;
                while ii != index_map[ii.index()] {
                    ii = index_map[ii.index()];
                }
                i.1 = ii;
            }
            nod.inputs.sort();
            nod.inputs.dedup();

            let Some(&same_node) = nod_map.get(&nod) else {
                nod_map.insert(nod.clone(), idx);
                continue;
            };

            changes.push((idx, same_node));

            let mut same_out = std::mem::take(&mut outputs[same_node.index()]);
            let this_out = &outputs[idx.index()];
            same_out.extend(this_out);

            for output in same_out.iter().copied() {
                if graph[output].is_output {
                    continue;
                }
                let output = index_map[output.index()];

                if !dedup_output.insert(output) {
                    continue;
                }
                next.push(output);
            }

            outputs[same_node.index()] = same_out;
        }

        for (from, to) in changes.drain(..) {
            index_map[from.index()] = to;
        }

        dedup_output.clear();
        current.clear();
        nod_map.clear();
        std::mem::swap(&mut current, &mut next);
    }

    let (new_graph, new_range_info) = from_nod_graph(graph, range_info, nods, index_map);
    *graph = new_graph;
    *range_info = new_range_info;
}

fn to_nod_graph(
    graph: &petgraph::prelude::StableGraph<crate::compile_graph::CompileNode, CompileLink>,
    range_info: &SSRangeInfo,
) -> (
    Vec<Nod>,
    Vec<HashSet<petgraph::prelude::NodeIndex>>,
    Vec<petgraph::prelude::NodeIndex>,
    Vec<petgraph::prelude::NodeIndex>,
) {
    let empty_nod = Nod {
        inputs: Default::default(),
        ty: NodeType::Constant,
        state: Default::default(),
    };
    let mut nods: Vec<Nod> = Vec::with_capacity(graph.node_bound());
    let mut outputs: Vec<HashSet<NodeIdx>> = Vec::with_capacity(graph.node_bound());
    let mut index_map: Vec<NodeIdx> = Vec::with_capacity(graph.node_bound());
    let mut next: Vec<NodeIdx> = Vec::new();

    for i in 0..graph.node_bound() {
        let idx = NodeIdx::new(i);
        if !graph.contains_node(idx) {
            nods.push(empty_nod.clone());
            outputs.push(Default::default());
            index_map.push(NodeIdx::end());
            continue;
        }
        index_map.push(idx);
        outputs.push(graph.neighbors_directed(idx, Outgoing).collect());

        let node = &graph[idx];
        if node.is_removable() {
            next.push(idx);
        }

        let is_bool = node.ty.is_bool();

        let mut inputs: Vec<(bool, NodeIdx, u8)> = graph
            .edges_directed(idx, Incoming)
            .map(|edge| {
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

                (is_side, source, ss_distance)
            })
            .collect();
        inputs.sort();

        let nod = Nod {
            inputs,
            ty: node.ty.clone(),
            state: node.state.clone(),
        };

        nods.push(nod);
    }
    (nods, outputs, index_map, next)
}

fn from_nod_graph(
    graph: &CompileGraph,
    range_info: &SSRangeInfo,
    nods: Vec<Nod>,
    index_map: Vec<NodeIdx>,
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

        for (side, mut old_source, ss_dist) in
            nods[old_target.index()].inputs.iter().cloned()
        {
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
    graph.remove_node(node);
}
