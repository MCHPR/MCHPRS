use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, NodeIdx, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::visit::NodeIndexable;
use petgraph::Direction;
use std::collections::HashMap;

pub struct ConstantCoalesce;

impl<W: World> Pass<W> for ConstantCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let mut constant_nodes = HashMap::new();

        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }

            if graph[idx].ty != NodeType::Constant {
                continue;
            }

            let ss = graph[idx].state.output_strength;

            match constant_nodes.get(&graph[idx].state.output_strength) {
                Some(&constant_idx) => {
                    let mut neighbors = graph.neighbors_directed(idx, Direction::Outgoing).detach();
                    while let Some(edge) = neighbors.next_edge(graph) {
                        let dest = graph.edge_endpoints(edge).unwrap().1;
                        let weight = graph.remove_edge(edge).unwrap();
                        graph.add_edge(constant_idx, dest, weight);
                    }
                    graph.remove_node(idx);
                }
                None => {
                    // Turn this node into a generic constant
                    graph[idx].block = None;
                    constant_nodes.insert(ss, idx);
                }
            }
        }
    }
}
