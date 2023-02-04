use super::Pass;
use crate::blocks::ComparatorMode as CComparatorMode;
use crate::redpiler::compile_graph::{
    CompileGraph, LinkType as CLinkType, NodeIdx, NodeType as CNodeType,
};
use crate::redpiler::{CompilerInput, CompilerOptions};
use crate::world::World;
use itertools::Itertools;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use redpiler_graph::{
    serialize, BlockPos, ComparatorMode, Link, LinkType, Node, NodeState, NodeType,
};
use std::collections::HashMap;
use std::fs;

fn convert_node(
    graph: &CompileGraph,
    node_idx: NodeIdx,
    nodes_map: &HashMap<NodeIdx, usize>,
) -> Node {
    let node = &graph[node_idx];

    let mut inputs = Vec::new();
    for edge in graph.edges_directed(node_idx, Direction::Incoming) {
        let idx = nodes_map[&edge.source()];
        let weight = edge.weight();
        inputs.push(Link {
            ty: match weight.ty {
                CLinkType::Default => LinkType::Default,
                CLinkType::Side => LinkType::Side,
            },
            weight: weight.ss,
            to: idx,
        });
    }

    let updates = graph
        .neighbors_directed(node_idx, Direction::Outgoing)
        .map(|idx| nodes_map[&idx])
        .collect();

    Node {
        ty: match node.ty {
            CNodeType::Repeater(delay) => NodeType::Repeater(delay),
            CNodeType::Torch => NodeType::Torch,
            CNodeType::Comparator(mode) => NodeType::Comparator(match mode {
                CComparatorMode::Compare => ComparatorMode::Compare,
                CComparatorMode::Subtract => ComparatorMode::Subtract,
            }),
            CNodeType::Lamp => NodeType::Lamp,
            CNodeType::Button => NodeType::Button,
            CNodeType::Lever => NodeType::Lever,
            CNodeType::PressurePlate => NodeType::PressurePlate,
            CNodeType::Trapdoor => NodeType::Trapdoor,
            CNodeType::Wire => NodeType::Wire,
            CNodeType::Constant => NodeType::Constant,
        },
        block: node.block.map(|(pos, id)| {
            (
                BlockPos {
                    x: pos.x,
                    y: pos.y,
                    z: pos.z,
                },
                id,
            )
        }),
        state: NodeState {
            output_strength: node.state.output_strength,
            powered: node.state.powered,
            repeater_locked: node.state.repeater_locked,
        },
        comparator_far_input: node.comparator_far_input,
        facing_diode: node.facing_diode,
        inputs,
        updates,
    }
}

pub struct ExportGraph;

impl<W: World> Pass<W> for ExportGraph {
    fn run_pass(&self, graph: &mut CompileGraph, _: &CompilerOptions, _: &CompilerInput<'_, W>) {
        let mut nodes_map = HashMap::with_capacity(graph.node_count());
        for node in graph.node_indices() {
            nodes_map.insert(node, nodes_map.len());
        }

        let nodes = graph
            .node_indices()
            .map(|idx| convert_node(graph, idx, &nodes_map))
            .collect_vec();

        fs::write("redpiler_graph.bc", serialize(nodes.as_slice()).unwrap()).unwrap();
    }

    fn should_run(&self, options: &CompilerOptions) -> bool {
        options.export
    }
}
