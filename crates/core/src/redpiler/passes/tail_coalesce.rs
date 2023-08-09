//! # [`TailCoalesce`]
//!
//! Turns all lines of comparators that only end in outputs into `Buffer` components.
//! The first comparator in the line is kept as is, to guarded the Buffer from non-`Normal` priority updates.

use super::Pass;
use crate::redpiler::compile_graph::{
    CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashSet;

pub struct TailCoalesce;

impl<W: World> Pass<W> for TailCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        // Set of all outputs
        let mut outputs = FxHashSet::default();
        // Set of all component lines that end in outputs
        let mut output_directed = FxHashSet::default();

        // Mark all components that transitively only connect to output components
        for idx in graph.node_indices() {
            if output_directed.contains(&idx) {
                continue;
            }
            let node = &graph[idx];
            if node.is_output
                && graph
                    .edges_directed(idx, Direction::Outgoing)
                    .next()
                    .is_none()
            {
                outputs.insert(idx);
                output_directed.insert(idx);

                let mut cur = idx;
                while let Ok(edge) = graph.edges_directed(cur, Direction::Incoming).exactly_one() {
                    let idx = edge.source();
                    if output_directed.contains(&idx) || edge.weight().ty != LinkType::Default {
                        break;
                    }
                    output_directed.insert(idx);
                    cur = idx;
                }
            }
        }
        // Remove certain output types for now, because of priority update order issue with buffers
        for idx in graph.node_indices() {
            let node = &graph[idx];
            if outputs.contains(&idx)
                && matches!(
                    node.ty,
                    NodeType::Lamp | NodeType::Torch | NodeType::Comparator(_)
                )
            {
                output_directed.remove(&idx);

                let mut cur = idx;
                while let Ok(edge) = graph.edges_directed(cur, Direction::Incoming).exactly_one() {
                    let idx = edge.source();
                    if !output_directed.contains(&idx) || edge.weight().ty != LinkType::Default {
                        break;
                    }
                    output_directed.remove(&idx);
                    cur = idx;
                }
            }
        }
        // Identify all lines to marked nodes (Comparator only for now)
        let comparator_lines = find_lines(graph, |idx, node| {
            matches!(node.ty, NodeType::Comparator(_))
                && !node.facing_diode
                && node.comparator_far_input == None
                && output_directed.contains(&idx)
        });

        // Replace all valid identified lines with buffers
        for line in comparator_lines {
            for line in line.chunks(16).filter(|l| l.len() >= 3) {
                // Keep first comparator as input guard
                let line = &line[1..line.len()];

                // Merge signal strength falloff
                let falloff: usize = line
                    .windows(2)
                    .map(|n| graph.find_edge(n[0], n[1]).unwrap())
                    .map(|idx| graph[idx].ss as usize)
                    .sum();

                let start = line[0];
                let end = line[line.len() - 1];

                // Remember incomming and outgoing connections
                let incomming = graph
                    .edges_directed(start, Direction::Incoming)
                    .map(|edge| (edge.source(), edge.weight().ss))
                    .collect_vec();
                let outgoing = graph
                    .edges_directed(end, Direction::Outgoing)
                    .map(|edge| (edge.target(), edge.weight().ss))
                    .collect_vec();
                // Remember state (assume the entire line is in the same state, use first component as source of truth)
                let precompile_output = graph[start].state.output_strength;

                for &idx in line.iter() {
                    graph.remove_node(idx);
                }

                let node = CompileNode {
                    ty: NodeType::Buffer(line.len() as u8),
                    block: None,
                    state: NodeState::comparator(precompile_output > 0, precompile_output),
                    facing_diode: false,
                    comparator_far_input: None,
                    is_input: false,
                    is_output: false,
                };
                let idx = graph.add_node(node);
                for input in incomming {
                    graph.add_edge(input.0, idx, CompileLink::default(falloff as u8 + input.1));
                }
                for output in outgoing {
                    graph.add_edge(idx, output.0, CompileLink::default(output.1));
                }
            }
        }
    }
}

/// Finds all lines of components that match the specified predicate.
/// Starts at some node idx and then matches for more line nodes in both directions.
fn find_lines<F>(graph: &CompileGraph, predicate: F) -> Vec<Vec<NodeIdx>>
where
    F: Fn(NodeIdx, &CompileNode) -> bool,
{
    let mut visited = FxHashSet::default();

    let mut lines = vec![];
    for idx in graph.node_indices() {
        if visited.contains(&idx) {
            continue;
        }
        visited.insert(idx);
        // Check if valid starting point
        if !(predicate(idx, &graph[idx]) && is_line(graph, idx, false, false)) {
            continue;
        }
        let mut line = vec![idx];
        // Match for line components backwards
        let mut cur = next(graph, idx, Direction::Incoming);
        while predicate(cur, &graph[cur]) && is_line(graph, cur, false, false) {
            line.push(cur);
            cur = next(graph, cur, Direction::Incoming);
        }
        // Add head (may have multiple inputs)
        if predicate(cur, &graph[cur]) && is_line(graph, cur, true, false) {
            line.push(cur);
        }
        line.reverse();

        // And then match forward
        let mut cur = next(graph, idx, Direction::Outgoing);
        while predicate(cur, &graph[cur]) && is_line(graph, cur, false, false) {
            line.push(cur);
            cur = next(graph, cur, Direction::Outgoing);
        }
        // Add tail (may have multiple outputs)
        if predicate(cur, &graph[cur]) && is_line(graph, cur, false, true) {
            line.push(cur);
        }
        
        // Mark all nodes that are part of the line as visited (nodes can only ever be part of a single line)
        visited.extend(line.iter());
        lines.push(line);
    }
    return lines;
}

/// Checks is a node could be safely removed and is part of a line.
/// Lines have exactly one incomming and outgoing default edge (except when `any_input` or `any_output` is true)
fn is_line(graph: &CompileGraph, idx: NodeIdx, any_input: bool, any_output: bool) -> bool {
    let node = &graph[idx];
    !node.is_input
        && !node.is_output
        && (if any_input {
            graph
                .edges_directed(idx, Direction::Incoming)
                .all(|e| e.weight().ty == LinkType::Default)
        } else {
            graph
                .edges_directed(idx, Direction::Incoming)
                .exactly_one()
                .map_or(false, |e| e.weight().ty == LinkType::Default)
        })
        && (if any_output {
            graph
                .edges_directed(idx, Direction::Outgoing)
                .all(|e| e.weight().ty == LinkType::Default)
        } else {
            graph
                .edges_directed(idx, Direction::Outgoing)
                .exactly_one()
                .map_or(false, |e| e.weight().ty == LinkType::Default)
        })
}

fn next(graph: &CompileGraph, idx: NodeIdx, dir: Direction) -> NodeIdx {
    graph.neighbors_directed(idx, dir).exactly_one().unwrap()
}
