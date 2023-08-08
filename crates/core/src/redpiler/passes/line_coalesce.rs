//! # [`LineCoalesce`]
//!
//! Isolates all otherwise unconnected lines of components and tries to combine these lines into a single `Buffer` component.
//! TODO: Expand explaination

use super::Pass;
use crate::redpiler::backend::bitqueue::BitQueue;
use crate::redpiler::compile_graph::{
    BufferMode, CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashSet;

pub struct LineCoalesce;

impl<W: World> Pass<W> for LineCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let comparator_lines = find_lines(graph, |node| {
            matches!(node.ty, NodeType::Comparator(_)) && !node.facing_diode
        });
        println!(
            "Histogram: {:?}",
            comparator_lines
                .iter()
                .counts_by(|l| l.len())
                .into_iter()
                .sorted_by_key(|(len, _)| *len)
        );

        for line in comparator_lines {
            let line = &line[1..line.len()];
            if line.len() <= 2 || line.len() >= BitQueue::MAX_NIBBLES {
                // TODO Split line
                continue;
            }
            let falloff: usize = line
                .windows(2)
                .map(|n| graph.find_edge(n[0], n[1]).unwrap())
                .map(|idx| graph[idx].ss as usize)
                .sum();
            if falloff >= 15 {
                // TODO Delete line
                continue;
            }
            let start = line[0];
            let end = line[line.len() - 1];
            let inputs = graph
                .edges_directed(start, Direction::Incoming)
                .map(|edge| (edge.source(), edge.weight().ss))
                .collect_vec();
            let outputs = graph
                .edges_directed(end, Direction::Outgoing)
                .map(|edge| (edge.target(), edge.weight().ss))
                .collect_vec();
            let precompile_output = graph[start].state.output_strength;

            for &idx in line.iter() {
                println!("Replaced: {:#?}", &graph[idx]);
                graph.remove_node(idx);
            }

            let node = CompileNode {
                ty: NodeType::Buffer(line.len() as u8, BufferMode::ComparatorOnly),
                block: None,
                state: NodeState::comparator(precompile_output > 0, precompile_output),
                facing_diode: false,
                comparator_far_input: None,
                is_input: false,
                is_output: false,
            };
            let idx = graph.add_node(node);
            for input in inputs {
                graph.add_edge(input.0, idx, CompileLink::default(falloff as u8 + input.1));
            }
            for output in outputs {
                graph.add_edge(idx, output.0, CompileLink::default(output.1));
            }
        }
    }
}

fn find_lines<F>(graph: &CompileGraph, predicate: F) -> Vec<Vec<NodeIdx>>
where
    F: Fn(&CompileNode) -> bool,
{
    let mut visited = FxHashSet::default();

    let mut lines = vec![];
    for idx in graph.node_indices() {
        if visited.contains(&idx) {
            continue;
        }
        visited.insert(idx);
        if !(is_line(graph, idx, false, false) && predicate(&graph[idx])) {
            continue;
        }
        let mut line = vec![idx];
        // Match backwards
        let mut cur = next(graph, idx, Direction::Incoming);
        while is_line(graph, cur, false, false) && predicate(&graph[cur]) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Incoming);
        }
        if is_line(graph, cur, true, false) && predicate(&graph[cur]) {
            line.push(cur);
            visited.insert(cur);
        }
        line.reverse();
        // Match forward
        let mut cur = next(graph, idx, Direction::Outgoing);
        while is_line(graph, cur, false, false) && predicate(&graph[cur]) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Outgoing);
        }
        if is_line(graph, cur, false, true) && predicate(&graph[cur]) {
            line.push(cur);
            visited.insert(cur);
        }
        lines.push(line);
    }
    return lines;
}

fn is_line(graph: &CompileGraph, idx: NodeIdx, any_input: bool, any_output: bool) -> bool {
    let node = &graph[idx];
    !node.is_input
        && !node.is_output
        && node.comparator_far_input == None
        && matches!(
            node.ty,
            NodeType::Repeater(1) | NodeType::Comparator(_) | NodeType::Torch
        )
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
