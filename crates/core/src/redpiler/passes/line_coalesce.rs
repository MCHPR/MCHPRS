//! # [`LineCoalesce`]
//!
//! Isolates all otherwise unconnected lines of components and tries to reduce them into `Buffer` components.
//! All lines that only consist of repeater, comparator and torch components can be reduced to a `HexBuffer` are of the form `-> falloff + [delay + falloff]* -> invert? ->`
//! For the special case, where such a line contains at least one repeater or torch, the signal strength information can be erased to create a `BinBuffer`
//! `-> comparator* -> [torch | repeater | comparator]* ->`
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

impl<W: World> Pass<W> for LineCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let lines = find_lines(graph);
        //println!("Histogram: {:?}", lines.iter().counts_by(|l| l.len()).into_iter().sorted_by_key(|(_, count)| *count));
        for line in lines.into_iter().filter(|v| v.len() > 2) {
            if let Some(mut prop) = try_match(graph, &line) {
                // TODO: Split when delay too big (64/256)
                if ((prop.delay >= 2 && !prop.invert) || prop.delay >= 3)
                    && prop.pre_falloff < 15
                    && prop.post_falloff < 15
                    && ((prop.delay < 256 && prop.binary) || (prop.delay < 64 && !prop.binary))
                {
                    let mut start = line[0];
                    let end = line[line.len() - 1];
                    for &idx in &line[1..(line.len() - 1)] {
                        graph.remove_node(idx);
                    }

                    let mut normalized_output = graph[start].state.output_strength;
                    if prop.invert {
                        normalized_output = if normalized_output > 0 { 0 } else { 15 };
                        let torch_node = CompileNode {
                            ty: NodeType::Torch,
                            block: None,
                            state: NodeState::comparator(normalized_output > 0, normalized_output),
                            facing_diode: false,
                            comparator_far_input: None,
                            is_input: false,
                            is_output: false,
                        };
                        let torch = graph.add_node(torch_node);
                        graph.add_edge(
                            start,
                            torch,
                            CompileLink {
                                ty: LinkType::Default,
                                ss: prop.pre_falloff as u8,
                            },
                        );
                        prop.delay -= 1;
                        prop.pre_falloff = 0;
                        start = torch;
                    }

                    let node = CompileNode {
                        ty: if prop.binary {
                            NodeType::BinBuffer(prop.delay as u8)
                        } else {
                            NodeType::HexBuffer(prop.delay as u8)
                        },
                        block: None,
                        state: NodeState::comparator(normalized_output > 0, normalized_output),
                        facing_diode: false,
                        comparator_far_input: None,
                        is_input: false,
                        is_output: false,
                    };
                    let buffer = graph.add_node(node);
                    graph.add_edge(
                        start,
                        buffer,
                        CompileLink {
                            ty: LinkType::Default,
                            ss: prop.pre_falloff as u8,
                        },
                    );
                    graph.add_edge(
                        buffer,
                        end,
                        CompileLink {
                            ty: LinkType::Default,
                            ss: prop.post_falloff as u8,
                        },
                    );
                }
            }
        }
    }
}

#[derive(Default)]
struct LineProperties {
    delay: usize,
    invert: bool,
    binary: bool,
    pre_falloff: usize,
    post_falloff: usize,
}

fn try_match(graph: &mut CompileGraph, line: &[NodeIdx]) -> Option<LineProperties> {
    if line.len() < 2 {
        return None;
    }
    let mut prop = LineProperties::default();

    let mut last = line[0];
    let mut index = 1;
    while index + 1 < line.len() {
        let idx = line[index];
        if let NodeType::Comparator(_) = &graph[idx].ty {
            prop.delay += 1;
            let edge = graph.find_edge(last, idx).unwrap();
            let link = &graph[edge];
            prop.pre_falloff += link.ss as usize;
        } else {
            break;
        }
        last = idx;
        index += 1;
    }
    while index + 1 < line.len() {
        let idx = line[index];
        let node = &graph[idx];
        let edge = graph.find_edge(last, idx).unwrap();
        let link = &graph[edge];
        match node.ty {
            NodeType::Repeater(1) => {
                prop.delay += 1;
                prop.post_falloff = 0;
                prop.binary = true;
            }
            NodeType::Torch => {
                prop.delay += 1;
                prop.post_falloff = 0;
                prop.invert = !prop.invert;
                prop.binary = true;
            }
            NodeType::Comparator(_) => {
                prop.delay += 1;
                prop.post_falloff += link.ss as usize;
            }
            _ => panic!("Unexpected node type: {:?}", node),
        };
        last = idx;
        index += 1;
    }
    let idx = line[index];
    let edge = graph.find_edge(last, idx).unwrap();
    let link = &graph[edge];
    prop.post_falloff += link.ss as usize;

    Some(prop)
}

fn find_lines(graph: &CompileGraph) -> Vec<Vec<NodeIdx>> {
    let mut visited = FxHashSet::default();

    let mut lines = vec![];
    for idx in graph.node_indices() {
        if visited.contains(&idx) {
            continue;
        }
        visited.insert(idx);
        if !is_line(graph, idx) {
            continue;
        }
        let mut line = vec![idx];
        // Match backwards
        let mut cur = next(graph, idx, Direction::Incoming);
        while is_line(graph, cur) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Incoming);
        }
        line.reverse();
        // Match forward
        let mut cur = next(graph, idx, Direction::Outgoing);
        while is_line(graph, cur) {
            line.push(cur);
            visited.insert(cur);
            cur = next(graph, cur, Direction::Outgoing);
        }
        lines.push(line);
    }
    return lines;
}

fn is_line(graph: &CompileGraph, idx: NodeIdx) -> bool {
    let edge_inc = graph.edges_directed(idx, Direction::Incoming).exactly_one();
    let edge_out = graph.edges_directed(idx, Direction::Outgoing).exactly_one();
    let node = &graph[idx];
    !node.is_input
        && !node.is_output
        && node.comparator_far_input == None
        && matches!(
            node.ty,
            NodeType::Repeater(1) | NodeType::Comparator(_) | NodeType::Torch
        )
        && edge_inc.is_ok_and(|e| e.weight().ty == LinkType::Default)
        && edge_out.is_ok_and(|e| e.weight().ty == LinkType::Default)
}

fn next(graph: &CompileGraph, idx: NodeIdx, dir: Direction) -> NodeIdx {
    graph.neighbors_directed(idx, dir).next().unwrap()
}
