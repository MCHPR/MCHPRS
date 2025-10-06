//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

mod compile;
mod execution_context;
mod flush;
mod node;
mod tick;
mod update;

use super::JITBackend;
use crate::backend::direct::execution_context::ExecutionContext;
use crate::compile_graph::CompileGraph;
use crate::task_monitor::TaskMonitor;
use crate::CompilerOptions;
use mchprs_blocks::blocks::{Block, ComparatorMode, Instrument};
use mchprs_blocks::BlockPos;
use mchprs_redstone::bool_to_ss;
use mchprs_world::{TickEntry, TickPriority, World};
use node::{Node, NodeId, NodeType, Nodes};
use rustc_hash::FxHashMap;
use std::fmt;
use std::sync::Arc;
use tracing::{debug, warn};

pub enum Event {
    NoteBlockPlay { noteblock_id: u16 },
}

#[derive(Default)]
pub struct DirectBackend {
    nodes: Nodes,
    blocks: Vec<Option<(BlockPos, Block)>>,
    pos_map: FxHashMap<BlockPos, NodeId>,
    noteblock_info: Vec<(BlockPos, Instrument, u32)>,
    execution_context: ExecutionContext,
    options: Options,
}

#[derive(Default)]
struct Options {
    is_io_only: bool,
}

impl DirectBackend {
    fn schedule_tick(&mut self, node_id: NodeId, delay: usize, priority: TickPriority) {
        self.execution_context
            .schedule_tick(node_id, delay, priority);
    }

    fn set_node(&mut self, node_id: NodeId, powered: bool, new_power: u8) {
        let node = &mut self.nodes[node_id];
        let old_power = node.output_power;

        if !node.is_frozen && !node.changed {
            self.execution_context.push_change(node_id);
            node.changed = true;
        }
        node.powered = powered;
        node.output_power = new_power;
        for i in 0..node.updates.len() {
            let node = &self.nodes[node_id];
            let update_link = unsafe { *node.updates.get_unchecked(i) };
            let side = update_link.side();
            let distance = update_link.ss();
            let update = update_link.node();

            let update_ref = &mut self.nodes[update];
            let inputs = if side {
                &mut update_ref.side_inputs
            } else {
                &mut update_ref.default_inputs
            };

            let old_power = old_power.saturating_sub(distance);
            let new_power = new_power.saturating_sub(distance);

            if old_power == new_power {
                continue;
            }

            // Safety: signal strength is never larger than 15
            unsafe {
                *inputs.ss_counts.get_unchecked_mut(old_power as usize) -= 1;
                *inputs.ss_counts.get_unchecked_mut(new_power as usize) += 1;
            }

            update::update_node(&mut self.execution_context, &mut self.nodes, update);
        }
    }
}

impl JITBackend for DirectBackend {
    fn inspect(&mut self, pos: BlockPos) {
        let Some(node_id) = self.pos_map.get(&pos) else {
            debug!("could not find node at pos {}", pos);
            return;
        };

        debug!("Node {:?}: {:#?}", node_id, self.nodes[*node_id]);
    }

    fn reset<W: World>(&mut self, world: &mut W) {
        self.flush_events(world);
        self.flush_block_changes(world);

        self.flush_scheduled_ticks(world);

        let _ = std::mem::take(&mut self.nodes);
        self.blocks.clear();
        self.pos_map.clear();
        self.noteblock_info.clear();
    }

    fn on_use_block(&mut self, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::Button => {
                if node.powered {
                    return;
                }
                self.schedule_tick(node_id, 10, TickPriority::Normal);
                self.set_node(node_id, true, 15);
            }
            NodeType::Lever => {
                self.set_node(node_id, !node.powered, bool_to_ss(!node.powered));
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.ty),
        }
    }

    fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::PressurePlate => {
                self.set_node(node_id, powered, bool_to_ss(powered));
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
        }
    }

    fn tick(&mut self) {
        let mut queues = self.execution_context.queues_this_tick();

        for node_id in queues.drain_iter() {
            self.tick_node(node_id);
        }

        self.execution_context.end_tick(queues);
    }

    fn flush<W: World>(&mut self, world: &mut W) {
        self.flush_events(world);
        self.flush_block_changes(world);
    }

    fn compile(
        &mut self,
        graph: CompileGraph,
        ticks: Vec<TickEntry>,
        options: &CompilerOptions,
        monitor: Arc<TaskMonitor>,
    ) {
        self.options.is_io_only = options.io_only;

        compile::compile(self, graph, ticks, monitor);

        if options.export_dot_graph {
            std::fs::write("backend_graph.dot", format!("{}", self)).unwrap();
        }
    }

    fn has_pending_ticks(&self) -> bool {
        self.execution_context.has_pending_ticks()
    }
}

/// Set node for use in `update`. None of the nodes here have usable output power,
/// so this function does not set that.
fn set_node(
    execution_context: &mut ExecutionContext,
    node_id: NodeId,
    node: &mut Node,
    powered: bool,
) {
    node.powered = powered;
    if !node.is_frozen && !node.changed {
        execution_context.push_change(node_id);
        node.changed = true;
    }
}

fn set_node_locked(
    execution_context: &mut ExecutionContext,
    node_id: NodeId,
    node: &mut Node,
    locked: bool,
) {
    node.locked = locked;
    if !node.is_frozen && !node.changed {
        execution_context.push_change(node_id);
        node.changed = true;
    }
}

fn schedule_tick(
    execution_context: &mut ExecutionContext,
    node_id: NodeId,
    node: &mut Node,
    delay: usize,
    priority: TickPriority,
) {
    node.pending_tick = true;
    execution_context.schedule_tick(node_id, delay, priority);
}

fn get_bool_input(node: &Node) -> bool {
    // During compilation its ensured all signal strength buckets add up to 255
    // So if and only if the zero bucket contains 255 is the input zero
    node.default_inputs.ss_counts[0] != 255
}

fn get_bool_side(node: &Node) -> bool {
    node.side_inputs.ss_counts[0] != 255
}

fn last_index_positive(array: &[u8; 16]) -> u32 {
    // Note: this might be slower on big-endian systems
    let value = u128::from_le_bytes(*array);
    if value == 0 {
        0
    } else {
        15 - (value.leading_zeros() >> 3)
    }
}

fn get_all_input(node: &Node) -> (u8, u8) {
    let input_power = last_index_positive(&node.default_inputs.ss_counts) as u8;

    let side_input_power = last_index_positive(&node.side_inputs.ss_counts) as u8;

    (input_power, side_input_power)
}

// This function is optimized for input values from 0 to 15 and does not work correctly outside that
// range
fn calculate_comparator_output(mode: ComparatorMode, input_strength: u8, power_on_sides: u8) -> u8 {
    let difference = input_strength.wrapping_sub(power_on_sides);
    if difference <= 15 {
        match mode {
            ComparatorMode::Compare => input_strength,
            ComparatorMode::Subtract => difference,
        }
    } else {
        0
    }
}

impl fmt::Display for DirectBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "digraph {{")?;
        for (id, node) in self.nodes.inner().iter().enumerate() {
            if matches!(node.ty, NodeType::Wire) {
                continue;
            }
            let label = match node.ty {
                NodeType::Repeater { delay, .. } => format!("Repeater({})", delay),
                NodeType::Torch => "Torch".to_string(),
                NodeType::Comparator { mode, .. } => format!(
                    "Comparator({})",
                    match mode {
                        ComparatorMode::Compare => "Cmp",
                        ComparatorMode::Subtract => "Sub",
                    }
                ),
                NodeType::Lamp => "Lamp".to_string(),
                NodeType::Button => "Button".to_string(),
                NodeType::Lever => "Lever".to_string(),
                NodeType::PressurePlate => "PressurePlate".to_string(),
                NodeType::Trapdoor => "Trapdoor".to_string(),
                NodeType::Wire => "Wire".to_string(),
                NodeType::Constant => format!("Constant({})", node.output_power),
                NodeType::NoteBlock { .. } => "NoteBlock".to_string(),
            };
            let pos = if let Some((pos, _)) = self.blocks[id] {
                format!("{}, {}, {}", pos.x, pos.y, pos.z)
            } else {
                "No Pos".to_string()
            };
            writeln!(f, "    n{} [ label = \"{}\\n({})\" ];", id, label, pos)?;
            for link in node.updates.iter() {
                let out_index = link.node().index();
                let distance = link.ss();
                let color = if link.side() { ",color=\"blue\"" } else { "" };
                writeln!(
                    f,
                    "    n{} -> n{} [ label = \"{}\"{} ];",
                    id, out_index, distance, color
                )?;
            }
        }
        writeln!(f, "}}")
    }
}
