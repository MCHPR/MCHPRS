//! # [`AnalogRepeaters`]
//!
//! This pass optimizes all instances of "analog repeaters", by replacing them with an equivalent comparator.
//! An analog repeater is a comparator, that is only connected to exactly 15 repeaters each with distances 0 counting to 14,
//! and then merging into only one comparator, each with again distances 0 counting to 14.

use super::Pass;
use crate::redpiler::compile_graph::{
    Annotations, CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use mchprs_blocks::blocks::ComparatorMode;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;

pub struct AnalogRepeaters;

impl<W: World> Pass<W> for AnalogRepeaters {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        'next: for i in 0..graph.node_bound() {
            let start_idx = NodeIdx::new(i);
            if !graph.contains_node(start_idx) {
                continue;
            }

            if !matches!(graph[start_idx].ty, NodeType::Comparator { .. }) {
                continue 'next;
            }
            let repeaters = graph
                .neighbors_directed(start_idx, Direction::Outgoing)
                .collect_vec();
            if repeaters.len() != 15 {
                continue 'next;
            }
            if !repeaters.iter().all(|&idx| {
                graph[idx].is_removable()
                    && matches!(
                        graph[idx].ty,
                        NodeType::Repeater {
                            delay: 1,
                            facing_diode: false
                        }
                    )
            }) {
                continue 'next;
            }
            let Ok(end_idx) = graph
                .neighbors_directed(repeaters[0], Direction::Outgoing)
                .exactly_one()
            else {
                continue 'next;
            };
            if !matches!(graph[end_idx].ty, NodeType::Comparator { .. }) {
                continue 'next;
            }
            let mut incomming = [false; 15];
            let mut outgoing = [false; 15];
            for &repeater in repeaters.iter() {
                let Ok(inc) = graph
                    .edges_directed(repeater, Direction::Incoming)
                    .exactly_one()
                else {
                    continue 'next;
                };
                let Ok(out) = graph
                    .edges_directed(repeater, Direction::Outgoing)
                    .exactly_one()
                else {
                    continue 'next;
                };
                if end_idx != out.target() {
                    continue 'next;
                }
                if inc.weight().ty != LinkType::Default || inc.weight().ty != LinkType::Default {
                    continue 'next;
                }
                if inc.weight().ss + out.weight().ss != 14 {
                    continue 'next;
                }
                incomming[inc.weight().ss as usize] = true;
                outgoing[out.weight().ss as usize] = true;
            }
            if incomming.into_iter().any(|x| !x) || outgoing.into_iter().any(|x| !x) {
                continue 'next;
            }

            for &idx in repeaters.iter() {
                graph.remove_node(idx);
            }

            let state = graph[start_idx].state.clone();
            let new_comparator = graph.add_node(CompileNode {
                ty: NodeType::Comparator {
                    mode: ComparatorMode::Compare,
                    far_input: None,
                    facing_diode: false,
                },
                block: None,
                state,
                is_input: false,
                is_output: false,
                annotations: Annotations::default(),
            });

            graph.add_edge(start_idx, new_comparator, CompileLink::default(0));
            graph.add_edge(new_comparator, end_idx, CompileLink::default(0));
        }
    }

    fn status_message(&self) -> &'static str {
        "Combining analog repeaters"
    }
}
