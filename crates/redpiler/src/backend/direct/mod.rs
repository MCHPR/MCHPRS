//! The direct backend lowers the compile graph to nodes and interprets their updates and ticks.

mod compile;
mod node;
mod tick;
mod update;

use std::{
    fmt::{self, Write},
    mem,
    sync::Arc,
};

use mchprs_blocks::{
    block_entities::BlockEntity,
    blocks::{Block, ComparatorMode, Instrument},
    BlockPos,
};
use mchprs_redstone::noteblock;
use mchprs_world::{TickEntry, TickPriority, World};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tracing::{debug, warn};

use self::node::{ForwardLinks, Node, NodeId, NodeType, Nodes};
use super::JITBackend;
use crate::compile_graph::{CompileGraph, SignalStrength};
use crate::{block_powered_mut, CompilerOptions, TaskMonitor};

#[derive(Default, Clone)]
struct Queues([Vec<NodeId>; TickScheduler::NUM_PRIORITIES]);

impl Queues {
    #[inline(always)]
    fn drain_each<F: FnMut(NodeId)>(&mut self, mut f: F) {
        for q in self.0.iter_mut() {
            for n in q.iter() {
                f(*n);
            }
            q.clear();
        }
    }
}

#[derive(Default)]
struct TickScheduler {
    queues_deque: [Queues; Self::NUM_QUEUES],
    pos: usize,
}

impl TickScheduler {
    const NUM_PRIORITIES: usize = 4;
    const NUM_QUEUES: usize = 16;

    fn reset<W: World>(&mut self, world: &mut W, blocks: &[impl AsRef<[(BlockPos, Block)]>]) {
        for (idx, queues) in self.queues_deque.iter().enumerate() {
            let delay = if self.pos >= idx {
                idx + Self::NUM_QUEUES
            } else {
                idx
            } - self.pos;
            for (entries, priority) in queues.0.iter().zip(Self::priorities()) {
                for node in entries {
                    let node_blocks = blocks[node.index()].as_ref();
                    if node_blocks.is_empty() {
                        warn!("Cannot schedule tick for node {:?} because block information is missing", node);
                        continue;
                    };
                    for (pos, _) in node_blocks.iter().copied() {
                        world.schedule_tick(pos, delay as u32, priority);
                    }
                }
            }
        }
        for queues in self.queues_deque.iter_mut() {
            for queue in queues.0.iter_mut() {
                queue.clear();
            }
        }
    }

    fn schedule_tick(&mut self, node: NodeId, delay: usize, priority: TickPriority) {
        self.queues_deque[(self.pos + delay) % Self::NUM_QUEUES].0[priority as usize].push(node);
    }

    fn queues_this_tick(&mut self) -> Queues {
        self.pos = (self.pos + 1) % Self::NUM_QUEUES;
        mem::take(&mut self.queues_deque[self.pos])
    }

    fn end_tick(&mut self, queues: Queues) {
        self.queues_deque[self.pos % Self::NUM_QUEUES] = queues;
    }

    fn priorities() -> [TickPriority; Self::NUM_PRIORITIES] {
        [
            TickPriority::Highest,
            TickPriority::Higher,
            TickPriority::High,
            TickPriority::Normal,
        ]
    }

    fn has_pending_ticks(&self) -> bool {
        for queues in &self.queues_deque {
            for queue in &queues.0 {
                if !queue.is_empty() {
                    return true;
                }
            }
        }
        false
    }
}

enum Event {
    NoteBlockPlay { noteblock_id: u16 },
}

#[derive(Default)]
pub struct DirectBackend {
    nodes: Nodes,
    forward_links: ForwardLinks,
    blocks: Vec<SmallVec<[(BlockPos, Block); 1]>>,
    pos_map: FxHashMap<BlockPos, NodeId>,
    scheduler: TickScheduler,
    events: Vec<Event>,
    noteblock_info: Vec<(SmallVec<[BlockPos; 1]>, Instrument, u8)>,
}

impl DirectBackend {
    fn set_power_and_propagate(&mut self, node_id: NodeId, power: SignalStrength) {
        let node = &mut self.nodes[node_id];
        let old_power = node.power;
        node.set_power(power);

        for forward_link in self.forward_links.get(&node.fwd_link_range) {
            let side = forward_link.side();
            let weight = forward_link.weight();
            let update = forward_link.node();

            let update_ref = &mut self.nodes[update];
            let inputs = if side {
                &mut update_ref.side_inputs
            } else {
                &mut update_ref.default_inputs
            };

            let old_input = old_power.saturating_sub(weight);
            let new_input = power.saturating_sub(weight);

            if old_input == new_input {
                continue;
            }

            inputs.update_power(old_input, new_input);

            update::update_node(
                &mut self.scheduler,
                &mut self.events,
                &mut self.nodes,
                update,
            );
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
        self.flush(world, false);
        self.scheduler.reset(world, &self.blocks);
        self.nodes = Nodes::default();
        self.blocks.clear();
        self.forward_links.clear();
        self.pos_map.clear();
        self.noteblock_info.clear();
        self.events.clear();
    }

    fn on_use_block(&mut self, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::Button => {
                if node.is_powered() {
                    return;
                }
                self.scheduler
                    .schedule_tick(node_id, 10, TickPriority::Normal);
                self.set_power_and_propagate(node_id, SignalStrength::MAX);
            }
            NodeType::Lever => {
                self.set_power_and_propagate(node_id, (!node.is_powered()).into());
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.ty),
        }
    }

    fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::PressurePlate => {
                self.set_power_and_propagate(node_id, powered.into());
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
        }
    }

    fn tick(&mut self) {
        let mut queues = self.scheduler.queues_this_tick();

        queues.drain_each(|node_id| {
            self.tick_node(node_id);
        });

        self.scheduler.end_tick(queues);
    }

    fn flush<W: World>(&mut self, world: &mut W, io_only: bool) {
        for event in self.events.drain(..) {
            match event {
                Event::NoteBlockPlay { noteblock_id } => {
                    let (positions, instrument, note) = &self.noteblock_info[noteblock_id as usize];
                    for pos in positions.iter().copied() {
                        noteblock::play_note(world, pos, *instrument, *note);
                    }
                }
            }
        }
        for (i, node) in self.nodes.inner_mut().iter_mut().enumerate() {
            if !node.changed || (io_only && !node.is_io) {
                continue;
            }
            node.changed = false;
            for (pos, block) in &mut self.blocks[i] {
                if let Some(powered) = block_powered_mut(block) {
                    *powered = node.is_powered()
                }
                if let Block::IronTrapdoor { open, .. } = block {
                    *open = node.is_powered();
                }
                if let Block::RedstoneWire(wire) = block {
                    wire.power = node.power.get()
                };
                if let Block::Repeater(repeater) = block {
                    repeater.locked = node.repeater_locked;
                }
                world.set_block(*pos, *block);
                if matches!(block, Block::Comparator(_)) {
                    world.set_block_entity(
                        *pos,
                        BlockEntity::Comparator {
                            output_strength: node.power.get(),
                        },
                    );
                }
            }
        }
    }

    fn compile(
        &mut self,
        graph: CompileGraph,
        ticks: Vec<TickEntry>,
        options: &CompilerOptions,
        monitor: Arc<TaskMonitor>,
    ) {
        compile::compile(self, graph, ticks, options, monitor);
    }

    fn has_pending_ticks(&self) -> bool {
        self.scheduler.has_pending_ticks()
    }
}

fn schedule_tick(
    scheduler: &mut TickScheduler,
    node_id: NodeId,
    node: &mut Node,
    delay: usize,
    priority: TickPriority,
) {
    node.pending_tick = true;
    scheduler.schedule_tick(node_id, delay, priority);
}

#[inline]
fn comparator_output_power(
    node: &Node,
    mode: ComparatorMode,
    far_input: Option<SignalStrength>,
) -> SignalStrength {
    let mut input_power = node.default_inputs.power();
    let side_power = node.side_inputs.power();
    if let Some(far_input) = far_input
        && input_power < SignalStrength::MAX
    {
        input_power = far_input;
    }
    let difference = input_power.get().wrapping_sub(side_power.get());
    if difference <= SignalStrength::MAX.get() {
        match mode {
            ComparatorMode::Compare => input_power,
            ComparatorMode::Subtract => SignalStrength::try_from(difference).unwrap(),
        }
    } else {
        SignalStrength::ZERO
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
                NodeType::Constant => format!("Constant({})", node.power),
                NodeType::NoteBlock { .. } => "NoteBlock".to_string(),
            };
            let pos = if !self.blocks[id].is_empty() {
                let mut string = String::new();
                for (idx, (pos, _)) in self.blocks[id].iter().enumerate() {
                    if idx != 0 {
                        write!(&mut string, "; ")?;
                    }
                    write!(&mut string, "{}, {}, {}", pos.x, pos.y, pos.z)?;
                }
                string
            } else {
                "No Pos".to_string()
            };
            writeln!(f, "    n{} [ label = \"{}\\n({})\" ];", id, label, pos)?;
            for link in self.forward_links.get(&node.fwd_link_range) {
                let out_index = link.node().index();
                let weight = link.weight();
                let color = if link.side() { ",color=\"blue\"" } else { "" };
                writeln!(
                    f,
                    "    n{} -> n{} [ label = \"{}\"{} ];",
                    id, out_index, weight, color
                )?;
            }
        }
        writeln!(f, "}}")
    }
}
