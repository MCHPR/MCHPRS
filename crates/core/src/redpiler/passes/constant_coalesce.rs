use std::collections::hash_map::Entry;

use super::Pass;
use crate::redpiler::compile_graph::{CompileGraph, CompileNode, NodeIdx, NodeState, NodeType};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use petgraph::unionfind::UnionFind;
use petgraph::visit::{EdgeRef, IntoEdgeReferences, NodeIndexable};
use petgraph::Direction;
use rustc_hash::FxHashMap;

pub struct ConstantCoalesce;

impl<W: World> Pass<W> for ConstantCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let mut vertex_sets = UnionFind::new(graph.node_bound());
        for edge in graph.edge_references() {
            let (src, dest) = (edge.source(), edge.target());
            let node = &graph[src];
            if node.ty != NodeType::Constant || !node.is_removable() {
                vertex_sets.union(graph.to_index(src), graph.to_index(dest));
            }
        }

        let mut constant_nodes = FxHashMap::default();
        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }
            let node = &graph[idx];
            if node.ty != NodeType::Constant || !node.is_removable() {
                continue;
            }
            let ss = node.state.output_strength;

            let mut neighbors = graph.neighbors_directed(idx, Direction::Outgoing).detach();
            while let Some((edge, dest)) = neighbors.next(graph) {
                let weight = graph.remove_edge(edge).unwrap();
                let subgraph_component = vertex_sets.find(graph.to_index(dest));

                let constant_idx = match constant_nodes.entry((subgraph_component, ss)) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        let constant_idx = graph.add_node(CompileNode {
                            ty: NodeType::Constant,
                            block: None,
                            state: NodeState::ss(ss),
                            is_input: false,
                            is_output: false,
                            annotations: Default::default(),
                        });
                        *entry.insert(constant_idx)
                    }
                };
                graph.add_edge(constant_idx, dest, weight);
            }
            graph.remove_node(idx);
        }
    }

    fn status_message(&self) -> &'static str {
        "Coalescing constants"
    }
}
