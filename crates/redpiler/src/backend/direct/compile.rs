use std::sync::Arc;

use itertools::Itertools;
use mchprs_blocks::{
    blocks::{Block, Instrument},
    BlockPos,
};
use mchprs_world::TickEntry;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tracing::trace;

use super::node::{ForwardLink, ForwardLinks, Node, NodeId, NodeInput, NodeType, Nodes};
use super::DirectBackend;
use crate::compile_graph::{
    CompileGraph, Direction, LinkType, NodeIdx, NodeType as CompileNodeType,
};
use crate::{CompilerOptions, TaskMonitor};

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
    noteblock_info: &mut Vec<(SmallVec<[BlockPos; 1]>, Instrument, u8)>,
    forward_links: &mut ForwardLinks,
    stats: &mut FinalGraphStats,
) -> Node {
    let node = &graph[node_idx];

    const MAX_INPUTS: usize = 255;

    let mut default_input_count = 0;
    let mut side_input_count = 0;

    let mut default_inputs = NodeInput::default();
    let mut side_inputs = NodeInput::default();
    for edge in graph.edges(node_idx, Direction::Incoming) {
        let link = edge.weight();
        let source = edge.source();
        let power = graph[source].state.power.saturating_sub(link.weight);
        match link.ty {
            LinkType::Default => {
                if default_input_count >= MAX_INPUTS {
                    panic!(
                        "Exceeded the maximum number of default inputs {}",
                        MAX_INPUTS
                    );
                }
                default_input_count += 1;
                default_inputs.power_counts[power.get() as usize] += 1;
            }
            LinkType::Side => {
                if side_input_count >= MAX_INPUTS {
                    panic!("Exceeded the maximum number of side inputs {}", MAX_INPUTS);
                }
                side_input_count += 1;
                side_inputs.power_counts[power.get() as usize] += 1;
            }
        }
    }
    stats.default_link_count += default_input_count;
    stats.side_link_count += side_input_count;

    default_inputs.power_counts[0] += (MAX_INPUTS - default_input_count) as u8;
    side_inputs.power_counts[0] += (MAX_INPUTS - side_input_count) as u8;

    let fwd_link_range = if node.ty != CompileNodeType::Constant {
        let new_links = graph
            .edges(node_idx, Direction::Outgoing)
            .sorted_by_key(|edge| nodes_map[&edge.target()])
            .into_group_map_by(|edge| std::mem::discriminant(&graph[edge.target()].ty))
            .into_values()
            .flatten()
            .map(|edge| unsafe {
                let idx = edge.target();
                let idx = nodes_map[&idx];
                assert!(idx < nodes_len);
                // Safety: bounds checked
                let target_id = NodeId::from_index(idx);

                let link = edge.weight();
                ForwardLink::new(target_id, link.ty == LinkType::Side, link.weight)
            });
        forward_links.extend(new_links)
    } else {
        Default::default()
    };
    stats.update_link_count += fwd_link_range.len();

    let ty = match &node.ty {
        CompileNodeType::Repeater {
            delay,
            facing_diode,
        } => NodeType::Repeater {
            delay: *delay,
            facing_diode: *facing_diode,
        },
        CompileNodeType::Torch => NodeType::Torch,
        CompileNodeType::Comparator {
            mode,
            far_input,
            facing_diode,
        } => NodeType::Comparator {
            mode: *mode,
            far_input: *far_input,
            facing_diode: *facing_diode,
        },
        CompileNodeType::Lamp => NodeType::Lamp,
        CompileNodeType::Button => NodeType::Button,
        CompileNodeType::Lever => NodeType::Lever,
        CompileNodeType::PressurePlate => NodeType::PressurePlate,
        CompileNodeType::Trapdoor => NodeType::Trapdoor,
        CompileNodeType::Wire => NodeType::Wire,
        CompileNodeType::Constant => NodeType::Constant,
        CompileNodeType::NoteBlock { instrument, note } => {
            let noteblock_id = noteblock_info.len().try_into().unwrap();
            noteblock_info.push((
                node.block.iter().copied().map(|(pos, _)| pos).collect(),
                *instrument,
                *note,
            ));
            NodeType::NoteBlock { noteblock_id }
        }
    };

    Node {
        ty,
        default_inputs,
        side_inputs,
        fwd_link_range,
        power: node.state.power,
        repeater_locked: node.state.repeater_locked,
        pending_tick: false,
        changed: false,
        is_io: node.is_input || node.is_output,
    }
}

pub fn compile(
    backend: &mut DirectBackend,
    graph: CompileGraph,
    ticks: Vec<TickEntry>,
    options: &CompilerOptions,
    _monitor: Arc<TaskMonitor>,
) {
    let mut nodes_map = FxHashMap::with_capacity_and_hasher(graph.node_count(), Default::default());
    for node_idx in graph.node_indices() {
        nodes_map.insert(node_idx, nodes_map.len());
    }
    let nodes_len = nodes_map.len();

    // Lower nodes
    let mut stats = FinalGraphStats::default();
    let nodes = graph
        .node_indices()
        .map(|idx| {
            compile_node(
                &graph,
                idx,
                nodes_len,
                &nodes_map,
                &mut backend.noteblock_info,
                &mut backend.forward_links,
                &mut stats,
            )
        })
        .collect();
    stats.nodes_bytes = nodes_len * std::mem::size_of::<Node>();
    trace!("{:#?}", stats);

    backend.blocks = graph
        .all_node_weights()
        .map(|node| {
            node.block
                .iter()
                .copied()
                .map(|(pos, id)| (pos, Block::from_id(id)))
                .collect()
        })
        .collect();
    backend.nodes = Nodes::new(nodes);

    // Create a mapping from block pos to backend NodeId
    for i in 0..backend.blocks.len() {
        for (pos, _) in backend.blocks[i].iter().copied() {
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
    if options.export_dot_graph {
        std::fs::write("backend_graph.dot", format!("{}", backend)).unwrap();
    }
}
