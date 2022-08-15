//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::blocks::{Block, ComparatorMode, RedstoneRepeater};
use crate::plot::PlotWorld;
use crate::redpiler::{bool_to_ss, CompileNode, LinkType, block_powered_mut};
use crate::world::World;
use itertools::Itertools;
use log::warn;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, TickPriority};
use nodes::{NodeId, Nodes};
use std::collections::HashMap;
use std::fmt;

mod nodes {
    use super::Node;
    use std::ops::{Index, IndexMut};

    #[derive(Debug, Copy, Clone)]
    pub struct NodeId(usize);

    impl NodeId {
        pub fn index(self) -> usize {
            self.0
        }

        /// Safety: index must be within bounds of nodes array
        pub unsafe fn from_index(index: usize) -> NodeId {
            NodeId(index)
        }
    }

    // This is Pretty Bad:tm: because one can create a NodeId using another instance of Nodes,
    // but at least some type system protection is better than none.
    #[derive(Default)]
    pub struct Nodes {
        nodes: Box<[Node]>,
    }

    impl Nodes {
        pub fn new(nodes: Box<[Node]>) -> Nodes {
            Nodes { nodes }
        }

        pub fn get(&self, idx: usize) -> NodeId {
            if self.nodes.get(idx).is_some() {
                NodeId(idx)
            } else {
                panic!("node index out of bounds: {}", idx)
            }
        }

        pub fn inner(&self) -> &[Node] {
            &self.nodes
        }

        pub fn inner_mut(&mut self) -> &mut [Node] {
            &mut self.nodes
        }

        pub fn into_inner(self) -> Box<[Node]> {
            self.nodes
        }
    }

    impl Index<NodeId> for Nodes {
        type Output = Node;

        // The index here MUST have been created by this instance, otherwise scary things will happen !
        fn index(&self, index: NodeId) -> &Self::Output {
            unsafe { self.nodes.get_unchecked(index.0) }
        }
    }

    impl IndexMut<NodeId> for Nodes {
        fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
            unsafe { self.nodes.get_unchecked_mut(index.0) }
        }
    }
}

#[derive(Debug, Clone)]
struct DirectLink {
    weight: u8,
    to: NodeId,
    ty: LinkType,
}

#[derive(Debug, Clone, Copy)]
enum NodeType {
    Repeater(u8),
    /// A non-locking repeater that doesn't face a diode
    SimpleRepeater,
    Torch,
    Comparator(ComparatorMode),
    Lamp,
    Button,
    Lever,
    PressurePlate,
    Trapdoor,
    Wire,
    Constant,
}

impl NodeType {
    fn new(block: Block) -> NodeType {
        match block {
            Block::RedstoneRepeater { repeater } => NodeType::Repeater(repeater.delay),
            Block::RedstoneComparator { comparator } => NodeType::Comparator(comparator.mode),
            Block::RedstoneTorch { .. } | Block::RedstoneWallTorch { .. } => NodeType::Torch,
            Block::RedstoneWire { .. } => NodeType::Wire,
            Block::StoneButton { .. } => NodeType::Button,
            Block::RedstoneLamp { .. } => NodeType::Lamp,
            Block::Lever { .. } => NodeType::Lever,
            Block::StonePressurePlate { .. } => NodeType::PressurePlate,
            Block::IronTrapdoor { .. } => NodeType::Trapdoor,
            Block::RedstoneBlock { .. } => NodeType::Constant,
            block if block.has_comparator_override() => NodeType::Constant,
            _ => panic!("Cannot determine node type for {:?}", block),
        }
    }

    fn is_io_block(self) -> bool {
        matches!(self, NodeType::Lamp | NodeType::Button | NodeType::Lever | NodeType::Trapdoor)
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pos: BlockPos,
    block: Block,
    ty: NodeType,
    inputs: Vec<DirectLink>,
    updates: Vec<NodeId>,
    facing_diode: bool,
    comparator_far_input: Option<u8>,

    /// Powered or lit
    powered: bool,
    /// Only for repeaters
    locked: bool,
    output_power: u8,
    changed: bool,
    pending_tick: bool,
}

impl Node {
    fn from_compile_node(node: CompileNode, nodes_len: usize) -> Self {
        let output_power = node.output_power();
        let powered = node.powered();
        let inputs = node
                .inputs
                .into_iter()
                .map(|link| {
                    assert!(link.end < nodes_len);
                    DirectLink {
                        weight: link.weight,
                        to: unsafe {
                            // Safety: bounds checked
                            NodeId::from_index(link.end)
                        },
                        ty: link.ty,
                    }
                })
                .collect_vec();
        let updates = node
                .updates
                .into_iter()
                .map(|idx| {
                    assert!(idx < nodes_len);
                    // Safety: bounds checked
                    unsafe { NodeId::from_index(idx) }
                })
                .collect();
        let ty = if matches!(node.state, Block::RedstoneRepeater { repeater: RedstoneRepeater { delay: 1, .. } }) && !inputs.iter().any(|input| input.ty == LinkType::Side) && !node.facing_diode {
            NodeType::SimpleRepeater
        } else {
            NodeType::new(node.state)
        };
        Node {
            pos: node.pos,
            block: node.state,
            ty,
            inputs,
            updates,
            powered,
            output_power,
            locked: match node.state {
                Block::RedstoneRepeater { repeater } => repeater.locked,
                _ => false,
            },
            facing_diode: node.facing_diode,
            comparator_far_input: node.comparator_far_input,
            pending_tick: false,
            changed: false,
        }
    }
}

struct RPTickEntry {
    priority: TickPriority,
    node: NodeId,
}

#[derive(Default)]
struct TickScheduler {
    queues: [Vec<RPTickEntry>; 4],
    current_queue: usize,
}

impl TickScheduler {
    fn reset(&mut self, plot: &mut PlotWorld, nodes: &Nodes) {
        for i in 0..4 {
            let queue_idx = (self.current_queue + i) % 4;
            let queue = &mut self.queues[queue_idx];
            for entry in queue.iter() {
                plot.schedule_tick(nodes[entry.node].pos, i as u32 + 1, entry.priority);
            }
            queue.clear();
        }
    }

    fn schedule_tick(&mut self, node: NodeId, delay: usize, priority: TickPriority) {
        let delay = self.current_queue + (delay - 1);
        self.queues[delay % 4].push(RPTickEntry { priority, node });
    }

    fn swap_queue(&mut self, with: &mut Vec<RPTickEntry>) {
        with.clear();
        std::mem::swap(&mut self.queues[self.current_queue], with);
        with.sort_by_key(|entry| entry.priority);
        self.current_queue += 1;
        self.current_queue %= 4;
    }
}

#[derive(Default)]
pub struct DirectBackend {
    nodes: Nodes,
    pos_map: HashMap<BlockPos, NodeId>,
    scheduler: TickScheduler,

    cached_queue: Option<Vec<RPTickEntry>>,
}

impl DirectBackend {
    fn schedule_tick(&mut self, node_id: NodeId, delay: usize, priority: TickPriority) {
        self.scheduler.schedule_tick(node_id, delay, priority);
    }

    fn set_node(&mut self, node_id: NodeId, powered: bool, new_power: u8) {
        let node = &mut self.nodes[node_id];
        node.changed = true;
        node.powered = powered;
        node.output_power = new_power;
        for i in 0..node.updates.len() {
            let update = self.nodes[node_id].updates[i];
            update_node(&mut self.scheduler, &mut self.nodes, update);
        }
        update_node(&mut self.scheduler, &mut self.nodes, node_id);
    }
}

impl JITBackend for DirectBackend {
    fn reset(&mut self, plot: &mut PlotWorld, io_only: bool) {
        self.scheduler.reset(plot, &self.nodes);

        let nodes = std::mem::take(&mut self.nodes);

        for node in nodes.into_inner().iter() {
            if matches!(node.ty, NodeType::Comparator(_)) {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_power,
                };
                plot.set_block_entity(node.pos, block_entity);
            }

            if io_only && !node.ty.is_io_block() {
                plot.set_block(node.pos, node.block);
            }
        }

        self.pos_map.clear();
    }

    fn on_use_block(&mut self, _plot: &mut PlotWorld, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::Button => {
                let powered = !node.powered;
                self.schedule_tick(node_id, 10, TickPriority::Normal);
                self.set_node(
                    node_id,
                    powered,
                    bool_to_ss(powered),
                );
            }
            NodeType::Lever => {
                self.set_node(node_id, !node.powered, bool_to_ss(!node.powered));
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.ty),
        }
    }

    fn set_pressure_plate(&mut self, _plot: &mut PlotWorld, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::PressurePlate => {
                self.set_node(
                    node_id,
                    powered,
                    bool_to_ss(powered),
                );
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
        }
    }

    fn tick(&mut self, _plot: &mut PlotWorld) {
        let mut queue = match self.cached_queue.take() {
            Some(queue) => queue,
            None => Vec::new(),
        };
        self.scheduler.swap_queue(&mut queue);

        for entry in &queue {
            let node_id = entry.node;
            self.nodes[node_id].pending_tick = false;
            let node = &self.nodes[node_id];

            let mut input_power = 0u8;
            let mut side_input_power = 0u8;
            for link in &node.inputs {
                let power = match link.ty {
                    LinkType::Default => &mut input_power,
                    LinkType::Side => &mut side_input_power,
                };
                *power = (*power).max(self.nodes[link.to].output_power.saturating_sub(link.weight));
            }

            match node.ty {
                NodeType::Repeater(_) => {
                    if node.locked {
                        continue;
                    }

                    let should_be_powered = input_power > 0;
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::SimpleRepeater => {
                    let should_be_powered = input_power > 0;
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::Torch => {
                    let should_be_off = input_power > 0;
                    let lit = node.powered;
                    if lit && should_be_off {
                        self.set_node(node_id, false, 0);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::Comparator(mode) => {
                    if let Some(far_override) = node.comparator_far_input {
                        if input_power < 15 {
                            input_power = far_override;
                        }
                    }
                    let new_strength =
                        calculate_comparator_output(mode, input_power, side_input_power);
                    let old_strength = node.output_power;
                    if new_strength != old_strength || mode == ComparatorMode::Compare {
                        let should_be_powered = comparator_should_be_powered(
                            mode,
                            input_power,
                            side_input_power,
                        );
                        let mut powered = node.powered;
                        if powered && !should_be_powered {
                            powered = false;
                        } else if !powered && should_be_powered {
                            powered = true;
                        }
                        self.set_node(
                            node_id,
                            powered,
                            new_strength,
                        );
                    }
                }
                NodeType::Lamp => {
                    let should_be_lit = input_power > 0;
                    if node.powered && !should_be_lit {
                        self.set_node(node_id, false, 0);
                    }
                }
                NodeType::Button => {
                    if node.powered {
                        self.set_node(node_id, false, 0);
                    }
                }
                _ => warn!("Node {:?} should not be ticked!", node.ty),
            }
        }
        self.cached_queue = Some(queue);
    }

    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {
        let nodes_len = nodes.len();
        let nodes = nodes
            .into_iter()
            .map(|cn| Node::from_compile_node(cn, nodes_len))
            .collect();
        self.nodes = Nodes::new(nodes);
        for (i, node) in self.nodes.inner().iter().enumerate() {
            self.pos_map.insert(node.pos, self.nodes.get(i));
        }
        for entry in ticks {
            if let Some(node) = self.pos_map.get(&entry.pos) {
                self.scheduler
                    .schedule_tick(*node, entry.ticks_left as usize, entry.tick_priority);
            }
        }
        // Dot file output
        // println!("{}", self);
    }

    fn flush(&mut self, plot: &mut PlotWorld, io_only: bool) {
        for node in self.nodes.inner_mut().iter_mut() {
            if node.changed && (!io_only || node.ty.is_io_block()) {
                if let Some(powered) = block_powered_mut(&mut node.block) {
                    *powered = node.powered
                }
                if let Block::RedstoneWire { wire, .. } = &mut node.block {
                    wire.power = node.output_power
                };
                plot.set_block(node.pos, node.block);
            }
            node.changed = false;
        }
    }
}

/// Set node for use in `update`. None of the nodes here have usable output power,
/// so this function does not set that.
fn set_node(node: &mut Node, powered: bool) {
    node.powered = powered;
    node.changed = true;
}

fn set_node_locked(node: &mut Node, locked: bool) {
    node.locked = locked;
    node.changed = true;
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

fn update_node(scheduler: &mut TickScheduler, nodes: &mut Nodes, node_id: NodeId) {
    let node = &nodes[node_id];

    let mut input_power = 0;
    let mut side_input_power = 0;
    for link in &node.inputs {
        let power = match link.ty {
            LinkType::Default => &mut input_power,
            LinkType::Side => &mut side_input_power,
        };
        *power = (*power).max(nodes[link.to].output_power.saturating_sub(link.weight));
    }

    match node.ty {
        NodeType::Repeater(delay) => {
            let node = &mut nodes[node_id];
            let should_be_locked = side_input_power > 0;
            if !node.locked && should_be_locked {
                set_node_locked(node, true);
            } else if node.locked && !should_be_locked {
                set_node_locked(node, false);
            }

            if !node.locked && !node.pending_tick {
                let should_be_powered = input_power > 0;
                if should_be_powered != node.powered {
                    let priority = if node.facing_diode {
                        TickPriority::Highest
                    } else if !should_be_powered {
                        TickPriority::Higher
                    } else {
                        TickPriority::High
                    };
                    schedule_tick(scheduler, node_id, node, delay as usize, priority);
                }
            }
        }
        NodeType::SimpleRepeater => {
            let should_be_powered = input_power > 0;
            if node.powered != should_be_powered && !node.pending_tick {
                let priority = if !should_be_powered {
                    TickPriority::Higher
                } else {
                    TickPriority::High
                };
                let node = &mut nodes[node_id];
                schedule_tick(scheduler, node_id, node, 1, priority);
            }
        }
        NodeType::Torch => {
            let lit = node.powered;
            if lit == (input_power > 0) && !node.pending_tick {
                let node = &mut nodes[node_id];
                schedule_tick(scheduler, node_id, node, 1, TickPriority::Normal);
            }
        }
        NodeType::Comparator(mode) => {
            if node.pending_tick {
                return;
            }
            if let Some(far_override) = node.comparator_far_input {
                if input_power < 15 {
                    input_power = far_override;
                }
            }
            let output_power =
                calculate_comparator_output(mode, input_power, side_input_power);
            let old_strength = node.output_power;
            if output_power != old_strength
                || node.powered
                    != comparator_should_be_powered(mode, input_power, side_input_power)
            {
                let priority = if node.facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                let node = &mut nodes[node_id];
                schedule_tick(scheduler, node_id, node, 1, priority);
            }
        }
        NodeType::Lamp => {
            let lit = node.powered;
            let should_be_lit = input_power > 0;
            let node = &mut nodes[node_id];
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node(node, true);
            }
        }
        NodeType::Trapdoor => {
            let should_be_powered = input_power > 0;
            if node.powered != should_be_powered {
                let node = &mut nodes[node_id];
                set_node(node, should_be_powered);
            }
        }
        NodeType::Wire => {
            if node.output_power != input_power {
                let node = &mut nodes[node_id];
                node.output_power = input_power;
                node.changed = true;
            }
        }
        _ => {} // panic!("Node {:?} should not be updated!", node.state),
    }
}

impl fmt::Display for DirectBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("digraph{")?;
        for (id, node) in self.nodes.inner().iter().enumerate() {
            if matches!(node.ty, NodeType::Wire) {
                continue;
            }
            write!(
                f,
                "n{}[label=\"{}\\n({}, {}, {})\"];",
                id,
                format!("{:?}", node.ty)
                    .split_whitespace()
                    .next()
                    .unwrap(),
                node.pos.x,
                node.pos.y,
                node.pos.z
            )?;
            for link in &node.inputs {
                let color = match link.ty {
                    LinkType::Default => "",
                    LinkType::Side => ",color=\"blue\"",
                };
                write!(
                    f,
                    "n{}->n{}[label=\"{}\"{}];",
                    link.to.index(),
                    id,
                    link.weight,
                    color
                )?;
            }
            // for update in &node.updates {
            //     write!(f, "n{}->n{}[style=dotted];", id, update)?;
            // }
        }
        f.write_str("}\n")
    }
}

fn comparator_should_be_powered(
    mode: ComparatorMode,
    input_strength: u8,
    power_on_sides: u8,
) -> bool {
    if input_strength == 0 {
        false
    } else if input_strength > power_on_sides {
        true
    } else {
        power_on_sides == input_strength && mode == ComparatorMode::Compare
    }
}

fn calculate_comparator_output(mode: ComparatorMode, input_strength: u8, power_on_sides: u8) -> u8 {
    if mode == ComparatorMode::Subtract {
        input_strength.saturating_sub(power_on_sides)
    } else if input_strength >= power_on_sides {
        input_strength
    } else {
        0
    }
}
