use super::Pass;
use crate::compile_graph::{CompileGraph, CompileLink, CompileNode, NodeIdx, NodeState, NodeType};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use petgraph::visit::NodeIndexable;
use petgraph::Direction;

pub struct ConstantCoalesce;

impl<W: World> Pass<W> for ConstantCoalesce {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let constant = graph.add_node(CompileNode {
            ty: NodeType::Constant,
            block: None,
            state: NodeState::ss(15),
            is_input: false,
            is_output: false,
            annotations: Default::default(),
        });

        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if !graph.contains_node(idx) {
                continue;
            }
            if idx == constant {
                continue;
            }
            let node = &graph[idx];
            if node.ty != NodeType::Constant || !node.is_removable() {
                continue;
            }
            let output_strength = node.state.output_strength;

            let mut neighbors = graph.neighbors_directed(idx, Direction::Outgoing).detach();
            while let Some((edge, dest)) = neighbors.next(graph) {
                let CompileLink { ty, ss } = graph[edge];
                let ss = ss + 15 - output_strength;
                if ss < 15 {
                    graph.add_edge(constant, dest, CompileLink::new(ty, ss));
                }
            }
            graph.remove_node(idx);
        }
    }

    fn status_message(&self) -> &'static str {
        "Coalescing constants"
    }
}
