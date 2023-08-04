//! # [`LineCoalesce`]
//!
// TODO

use super::Pass;
use crate::redpiler::compile_graph::{
    CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::Direction;
use rustc_hash::FxHashSet;

pub struct LineCoalesce;

const QUEUE_BITS: usize = 256;

impl<W: World> Pass<W> for LineCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        coalesce_1tick_repeater_lines(graph);
        coalesce_comparator_lines(graph);
    }
}

fn coalesce_1tick_repeater_lines(graph: &mut CompileGraph) {
    let lines = find_lines(graph, |x| x.ty == NodeType::Repeater(1));
    for line in lines {
        for line in line.chunks(QUEUE_BITS + 2) {
            if line.len() >= 4 {
                let start = line[0];
                let end = line[line.len() - 1];
                let delay = (line.len() - 2) as u8;
                let buffer = graph.add_node(CompileNode {
                    ty: NodeType::BinBuffer(delay),
                    block: None,
                    state: NodeState::default(),
                    facing_diode: false,
                    comparator_far_input: None,
                    is_input: false,
                    is_output: false,
                });
                graph.add_edge(
                    start,
                    buffer,
                    CompileLink {
                        ty: LinkType::Default,
                        ss: 0,
                    },
                );
                graph.add_edge(
                    buffer,
                    end,
                    CompileLink {
                        ty: LinkType::Default,
                        ss: 0,
                    },
                );
                for &idx in &line[1..(line.len() - 1)] {
                    graph.remove_node(idx);
                }
            }
        }
    }
}

fn coalesce_comparator_lines(graph: &mut CompileGraph) {
    let lines = find_lines(graph, |x| {
        matches!(x.ty, NodeType::Comparator(_)) && matches!(x.comparator_far_input, None)
    });
    for line in lines {
        for line in line.chunks(QUEUE_BITS / 4 + 2) {
            if line.len() >= 4 {
                let start = line[0];
                let end = line[line.len() - 1];
                let delay = (line.len() - 2) as u8;
                let falloff: u8 = line
                    .windows(2)
                    .map(|w| graph.find_edge(w[0], w[1]).unwrap())
                    .map(|e| graph[e].ss)
                    .sum();
                if falloff >= 15 {
                    // TODO: Remove entire line
                    continue;
                }
                let buffer = graph.add_node(CompileNode {
                    ty: NodeType::HexBuffer(delay),
                    block: None,
                    state: NodeState::default(),
                    facing_diode: false,
                    comparator_far_input: None,
                    is_input: false,
                    is_output: false,
                });
                graph.add_edge(
                    start,
                    buffer,
                    CompileLink {
                        ty: LinkType::Default,
                        ss: 0,
                    },
                );
                graph.add_edge(
                    buffer,
                    end,
                    CompileLink {
                        ty: LinkType::Default,
                        ss: falloff,
                    },
                );
                for &idx in &line[1..(line.len() - 1)] {
                    graph.remove_node(idx);
                }
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
        if !(predicate(&graph[idx]) && is_line(graph, idx)) {
            continue;
        }
        let mut line = vec![idx];
        // Match backwards
        let mut cur = next(graph, idx, Direction::Incoming);
        while predicate(&graph[idx]) && is_line(graph, cur) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Incoming);
        }
        line.reverse();
        // Match forward
        let mut cur = next(graph, idx, Direction::Outgoing);
        while predicate(&graph[idx]) && is_line(graph, cur) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Outgoing);
        }
        lines.push(line);
    }
    println!("Identified {} lines", lines.len());
    println!("Histogram: {:?}", lines.iter().counts_by(|l| l.len()));
    return lines;
}

fn is_line(graph: &CompileGraph, idx: NodeIdx) -> bool {
    let edge_inc = graph.edges_directed(idx, Direction::Incoming).exactly_one();
    let edge_out = graph.edges_directed(idx, Direction::Outgoing).exactly_one();
    let node = &graph[idx];
    !node.is_input && !node.is_output
        && edge_inc.is_ok_and(|e| e.weight().ty == LinkType::Default)
        && edge_out.is_ok_and(|e| e.weight().ty == LinkType::Default)
}

fn next(graph: &CompileGraph, idx: NodeIdx, dir: Direction) -> NodeIdx {
    graph.neighbors_directed(idx, dir).next().unwrap()
}
