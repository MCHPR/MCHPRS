use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx};
use crate::redpiler::TaskMonitor;
use mchprs_blocks::blocks::Block;
use mchprs_world::TickEntry;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::sync::Arc;
use tracing::trace;

use super::node::{ForwardLink, Node, NodeId, NodeInput, NodeType, Nodes};
use super::DirectBackend;

#[derive(Debug, Default)]
struct FinalGraphStats {
    update_link_count: usize,
    side_link_count: usize,
    default_link_count: usize,
    nodes_bytes: usize,
}

fn compile_node(
    graph: &CompileGraph,
    node_idx: NodeIdx,
    nodes_len: usize,
    nodes_map: &FxHashMap<NodeIdx, usize>,
    stats: &mut FinalGraphStats,
) -> Node {
    let node = &graph[node_idx];

    const MAX_INPUTS: usize = 255;

    let mut default_input_count = 0;
    let mut side_input_count = 0;

    let mut default_inputs = NodeInput { ss_counts: [0; 16] };
    let mut side_inputs = NodeInput { ss_counts: [0; 16] };
    for edge in graph.edges_directed(node_idx, Direction::Incoming) {
        let weight = edge.weight();
        let distance = weight.ss;
        let source = edge.source();
        let ss = graph[source].state.output_strength.saturating_sub(distance);
        match weight.ty {
            LinkType::Default => {
                if default_input_count >= MAX_INPUTS {
                    panic!(
                        "Exceeded the maximum number of default inputs {}",
                        MAX_INPUTS
                    );
                }
                default_input_count += 1;
                default_inputs.ss_counts[ss as usize] += 1;
            }
            LinkType::Side => {
                if side_input_count >= MAX_INPUTS {
                    panic!("Exceeded the maximum number of side inputs {}", MAX_INPUTS);
                }
                side_input_count += 1;
                side_inputs.ss_counts[ss as usize] += 1;
            }
        }
    }
    stats.default_link_count += default_input_count;
    stats.side_link_count += side_input_count;

    use crate::redpiler::compile_graph::NodeType as CNodeType;
    let updates = if node.ty != CNodeType::Constant {
        graph
            .edges_directed(node_idx, Direction::Outgoing)
            .map(|edge| unsafe {
                let idx = edge.target();
                let idx = nodes_map[&idx];
                assert!(idx < nodes_len);
                // Safety: bounds checked
                let target_id = NodeId::from_index(idx);

                let weight = edge.weight();
                ForwardLink::new(target_id, weight.ty == LinkType::Side, weight.ss)
            })
            .collect()
    } else {
        SmallVec::new()
    };
    stats.update_link_count += updates.len();

    let ty = match node.ty {
        CNodeType::Repeater(delay) => {
            if side_input_count == 0 {
                NodeType::SimpleRepeater(delay)
            } else {
                NodeType::Repeater(delay)
            }
        }
        CNodeType::Torch => NodeType::Torch,
        CNodeType::Comparator(mode) => NodeType::Comparator(mode),
        CNodeType::Lamp => NodeType::Lamp,
        CNodeType::Button => NodeType::Button,
        CNodeType::Lever => NodeType::Lever,
        CNodeType::PressurePlate => NodeType::PressurePlate,
        CNodeType::Trapdoor => NodeType::Trapdoor,
        CNodeType::Wire => NodeType::Wire,
        CNodeType::Constant => NodeType::Constant,
    };

    Node {
        ty,
        default_inputs,
        side_inputs,
        updates,
        powered: node.state.powered,
        output_power: node.state.output_strength,
        locked: node.state.repeater_locked,
        facing_diode: node.facing_diode,
        comparator_far_input: node.comparator_far_input,
        pending_tick: false,
        changed: false,
    }
}

pub fn compile(
    backend: &mut DirectBackend,
    graph: CompileGraph,
    ticks: Vec<TickEntry>,
    _monitor: Arc<TaskMonitor>,
) {
    // Create a mapping from compile to backend node indices
    let mut nodes_map = FxHashMap::with_capacity_and_hasher(graph.node_count(), Default::default());
    for node in graph.node_indices() {
        nodes_map.insert(node, nodes_map.len());
    }
    let nodes_len = nodes_map.len();

    // Lower nodes
    let mut stats = FinalGraphStats::default();
    let nodes = graph
        .node_indices()
        .map(|idx| compile_node(&graph, idx, nodes_len, &nodes_map, &mut stats))
        .collect();
    stats.nodes_bytes = nodes_len * std::mem::size_of::<Node>();
    trace!("{:#?}", stats);

    backend.blocks = graph
        .node_weights()
        .map(|node| node.block.map(|(pos, id)| (pos, Block::from_id(id))))
        .collect();
    backend.nodes = Nodes::new(nodes);

    // Create a mapping from block pos to backend NodeId
    for i in 0..backend.blocks.len() {
        if let Some((pos, _)) = backend.blocks[i] {
            backend.pos_map.insert(pos, backend.nodes.get(i));
        }
    }

    // Schedule backend ticks
    for entry in ticks {
        if let Some(node) = backend.pos_map.get(&entry.pos) {
            backend
                .scheduler
                .schedule_tick(*node, entry.ticks_left as usize, entry.tick_priority);
            backend.nodes[*node].pending_tick = true;
        }
    }
    // Dot file output
    // println!("{}", self);
}
