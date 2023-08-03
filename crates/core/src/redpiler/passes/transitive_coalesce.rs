//! # [`TransitiveCoalesce`]
//!
// TODO

use std::collections::VecDeque;

use super::Pass;
use crate::redpiler::compile_graph::{
    CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeState, NodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::Direction;
use rustc_hash::FxHashSet;

pub struct TransitiveCoalesce;

// TODO Increase this to 255 after changing amount of tick queues
const MAX_DELAY: usize = 15;

impl<W: World> Pass<W> for TransitiveCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let repeater_lines = find_lines(graph, MAX_DELAY + 2, |x| x.ty == NodeType::Repeater(1));
        for mut line in repeater_lines {
            if line.len() > 3 {
                let delay = (line.len() - 2) as u8;
                let buffer = graph.add_node(CompileNode {
                    ty: NodeType::Buffer(delay),
                    block: None,
                    state: NodeState::default(),
                    facing_diode: false,
                    comparator_far_input: None,
                    is_input: false,
                    is_output: false,
                });
                let start = line.pop_front().unwrap();
                let end = line.pop_back().unwrap();
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
                for idx in line {
                    graph.remove_node(idx);
                }
            }
        }
    }
}

fn find_lines<F>(graph: &CompileGraph, max_length: usize, predicate: F) -> Vec<VecDeque<NodeIdx>>
where
    F: Fn(&CompileNode) -> bool,
{
    let mut visited = FxHashSet::default();

    let mut lines = vec![];
    for idx in graph.node_indices() {
        if visited.contains(&idx) {
            println!("Skipped: {:?}", idx);
            continue;
        }
        visited.insert(idx);
        if !(predicate(&graph[idx]) && is_line(graph, idx)) {
            println!("Not a line: {:?}", idx);
            continue;
        }
        println!("Found new line: {:?}", idx);
        let mut line = vec![idx];
        // Match backwards
        let mut cur = next(graph, idx, Direction::Incoming);
        while predicate(&graph[idx]) && is_line(graph, cur) {
            println!("Backward: {:?}", cur);
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Incoming);
        }
        line.reverse();
        // Match forward
        let mut cur = next(graph, idx, Direction::Outgoing);
        while predicate(&graph[idx]) && is_line(graph, cur) {
            println!("Forward: {:?}", cur);
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Outgoing);
        }
        lines.push(line);
    }
    println!("Identified {} lines", lines.len());
    println!("Histogram: {:?}", lines.iter().counts_by(|l| l.len()));
    // TODO: In an ideal world this would just be a simple iterator
    lines
        .into_iter()
        .flat_map(|l| {
            l.chunks(max_length)
                .map(|chunk| VecDeque::from(chunk.to_vec()))
                .collect_vec()
        })
        .collect_vec()
}

fn is_line(graph: &CompileGraph, idx: NodeIdx) -> bool {
    let edge_inc = graph.edges_directed(idx, Direction::Incoming).exactly_one();
    let edge_out = graph.edges_directed(idx, Direction::Outgoing).exactly_one();
    matches!(graph[idx].ty, NodeType::Repeater(_))
        && !graph[idx].is_output
        && edge_inc.is_ok_and(|e| e.weight().ty == LinkType::Default)
        && edge_out.is_ok_and(|e| e.weight().ty == LinkType::Default)
}

fn next(graph: &CompileGraph, idx: NodeIdx, dir: Direction) -> NodeIdx {
    graph.neighbors_directed(idx, dir).next().unwrap()
}
