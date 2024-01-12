use super::Pass;
use crate::redpiler::compile_graph::{weakly_connected_components, CompileGraph, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::Direction;
use rustc_hash::FxHashMap;

pub struct ConstantCoalesce;

impl<W: World> Pass<W> for ConstantCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let components = weakly_connected_components(graph);
        for component in components {
            let mut constant_nodes = FxHashMap::default();

            for idx in component {
                let node = &mut graph[idx];
                if node.ty != NodeType::Constant || !node.is_removable() {
                    continue;
                }

                let ss = node.state.output_strength;

                match constant_nodes.get(&ss) {
                    Some(&constant_idx) => {
                        let mut neighbors =
                            graph.neighbors_directed(idx, Direction::Outgoing).detach();
                        while let Some(edge) = neighbors.next_edge(graph) {
                            let dest = graph.edge_endpoints(edge).unwrap().1;
                            let weight = graph.remove_edge(edge).unwrap();
                            graph.add_edge(constant_idx, dest, weight);
                        }
                        graph.remove_node(idx);
                    }
                    None => {
                        // Turn this node into a generic constant
                        node.block = None;
                        constant_nodes.insert(ss, idx);
                    }
                }
            }
        }
    }

    fn status_message(&self) -> &'static str {
        "Coalescing constants"
    }
}
