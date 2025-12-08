use crate::compile_graph::{CompileGraph, LinkType, NodeIdx};
use crate::{CompilerOptions, TaskMonitor};
use itertools::Itertools;
use mchprs_blocks::blocks::{Block, Instrument};
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;
use petgraph::visit::EdgeRef;
use petgraph::Direction::{self, Outgoing};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tracing::trace;

use super::node::{ForwardLink, Node, NodeId, NodeInput, NodeType, Nodes, NonMaxU8};
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
    noteblock_info: &mut Vec<(BlockPos, Instrument, u32)>,
    forward_links: &mut Vec<ForwardLink>,
    stats: &mut FinalGraphStats,
    out_nodes: &mut Vec<Node>,
) {
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

    // Make sure signal strength buckets add up to 255 so we can easily check for all zeros in
    // get_bool_input
    default_inputs.ss_counts[0] += (MAX_INPUTS - default_input_count) as u8;
    side_inputs.ss_counts[0] += (MAX_INPUTS - side_input_count) as u8;

    use crate::compile_graph::NodeType as CNodeType;
    forward_links.clear();
    if node.ty != CNodeType::Constant {
        let new_links = graph
            .edges_directed(node_idx, Direction::Outgoing)
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

                let weight = edge.weight();
                ForwardLink::new(target_id, weight.ty == LinkType::Side, weight.ss)
            });
        forward_links.extend(new_links);
    };
    stats.update_link_count += forward_links.len();

    let ty = match &node.ty {
        CNodeType::Repeater {
            delay,
            facing_diode,
        } => NodeType::Repeater {
            delay: *delay,
            facing_diode: *facing_diode,
        },
        CNodeType::Torch => NodeType::Torch,
        CNodeType::Comparator {
            mode,
            far_input,
            facing_diode,
        } => NodeType::Comparator {
            mode: *mode,
            far_input: far_input.map(|value| NonMaxU8::new(value).unwrap()),
            facing_diode: *facing_diode,
        },
        CNodeType::Lamp => NodeType::Lamp,
        CNodeType::Button => NodeType::Button,
        CNodeType::Lever => NodeType::Lever,
        CNodeType::PressurePlate => NodeType::PressurePlate,
        CNodeType::Trapdoor => NodeType::Trapdoor,
        CNodeType::Wire => NodeType::Wire,
        CNodeType::Constant => NodeType::Constant,
        CNodeType::NoteBlock { instrument, note } => {
            let noteblock_id = noteblock_info.len().try_into().unwrap();
            noteblock_info.push((node.block.unwrap().0, *instrument, *note));
            NodeType::NoteBlock { noteblock_id }
        }
    };

    let fwd_link_len = forward_links.len();
    assert!(fwd_link_len < u16::MAX as usize);

    // Safety: These are simply placeholder values and should never be read
    let mut first_links = [ForwardLink::new(unsafe { NodeId::from_index(0) }, false, 0); 5];
    let num_first = fwd_link_len.min(first_links.len());
    first_links[..num_first].copy_from_slice(&forward_links[..num_first]);

    let node = Node {
        ty,
        default_inputs,
        side_inputs,
        fwd_link_len: fwd_link_len as u16,
        powered: node.state.powered,
        output_power: node.state.output_strength,
        locked: node.state.repeater_locked,
        pending_tick: false,
        changed: false,
        is_io: node.is_input || node.is_output,
        fwd_links: first_links,
    };

    let num_link_blocks = node.forward_link_blocks();

    out_nodes.reserve(1 + num_link_blocks);
    out_nodes.push(node);

    let node = out_nodes.last_mut().unwrap();
    // Safety: Capacity is previously reserved
    // Safety: Node.num_link_blocks allows skipping over the ForwardLink's when iterating
    unsafe {
        node.forward_links_mut()[num_first..].copy_from_slice(&forward_links[num_first..]);
        out_nodes.set_len(out_nodes.len() + num_link_blocks);
    }
}

pub fn compile(
    backend: &mut DirectBackend,
    graph: CompileGraph,
    ticks: Vec<TickEntry>,
    options: &CompilerOptions,
    _monitor: Arc<TaskMonitor>,
) {
    backend.blocks = Vec::with_capacity(graph.node_count());

    // Create a mapping from compile to backend node indices
    let mut nodes_map = FxHashMap::with_capacity_and_hasher(graph.node_count(), Default::default());
    let mut nodes_len = 0;
    for node in graph.node_indices() {
        nodes_map.insert(node, nodes_len);

        let outgoing = if graph[node].ty == crate::compile_graph::NodeType::Constant {
            0
        } else {
            graph.neighbors_directed(node, Outgoing).count()
        };
        let extra_nodes = Node::forward_link_blocks_for(outgoing);
        nodes_len += 1 + extra_nodes;

        let block = graph[node].block.map(|(pos, id)| (pos, Block::from_id(id)));
        backend.blocks.push(block);

        for _ in 0..extra_nodes {
            backend.blocks.push(None);
        }
    }

    // Lower nodes
    let mut stats = FinalGraphStats::default();
    let mut nodes = Vec::new();
    let mut forward_links = Vec::new();
    for idx in graph.node_indices() {
        compile_node(
            &graph,
            idx,
            nodes_len,
            &nodes_map,
            &mut backend.noteblock_info,
            &mut forward_links,
            &mut stats,
            &mut nodes,
        );
    }
    stats.nodes_bytes = nodes_len * std::mem::size_of::<Node>();
    trace!("{:#?}", stats);

    assert_eq!(nodes.len(), nodes_len);

    backend.nodes = Nodes::new(nodes.into_boxed_slice());

    // Create a mapping from block pos to backend NodeId
    for i in backend.nodes.ids() {
        if let Some((pos, _)) = backend.blocks[i.index()] {
            backend.pos_map.insert(pos, backend.nodes.get(i.index()));
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
