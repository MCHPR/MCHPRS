//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::blocks::{Block, ComparatorMode};
use crate::plot::PlotWorld;
use crate::redpiler::{bool_to_ss, CompileNode, LinkType};
use crate::world::World;
use log::warn;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, TickPriority};
use nodes::{NodeId, Nodes};
use std::collections::HashMap;
use std::fmt;

fn is_io_block(block: Block) -> bool {
    matches!(
        block,
        Block::RedstoneLamp { .. }
            | Block::Lever { .. }
            | Block::StoneButton { .. }
            | Block::StonePressurePlate { .. }
            | Block::IronTrapdoor { .. }
    )
}

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

#[derive(Debug, Clone)]
pub struct Node {
    pos: BlockPos,
    inputs: Vec<DirectLink>,
    facing_diode: bool,
    comparator_far_input: Option<u8>,

    state: Block,
    updates: Vec<NodeId>,
    output_power: u8,
    changed: bool,
    pending_tick: bool,
}

impl Node {
    fn from_compile_node(node: CompileNode, nodes_len: usize) -> Self {
        let output_power = node.output_power();
        Node {
            pos: node.pos,
            state: node.state,
            inputs: node
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
                .collect(),
            updates: node
                .updates
                .into_iter()
                .map(|idx| {
                    assert!(idx < nodes_len);
                    // Safety: bounds checked
                    unsafe { NodeId::from_index(idx) }
                })
                .collect(),
            output_power,
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

    fn set_node(&mut self, node_id: NodeId, new_block: Block, new_power: u8) {
        let node = &mut self.nodes[node_id];
        node.state = new_block;
        node.changed = true;
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
            if let Block::RedstoneComparator { .. } = node.state {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_power,
                };
                plot.set_block_entity(node.pos, block_entity);
            }

            if io_only && !is_io_block(node.state) {
                plot.set_block(node.pos, node.state);
            }
        }

        self.pos_map.clear();
    }

    fn on_use_block(&mut self, _plot: &mut PlotWorld, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.state {
            Block::StoneButton { mut button } => {
                button.powered = !button.powered;
                self.schedule_tick(node_id, 10, TickPriority::Normal);
                self.set_node(
                    node_id,
                    Block::StoneButton { button },
                    bool_to_ss(button.powered),
                );
            }
            Block::Lever { mut lever } => {
                lever.powered = !lever.powered;
                self.set_node(node_id, Block::Lever { lever }, bool_to_ss(lever.powered));
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.state),
        }
    }

    fn set_pressure_plate(&mut self, _plot: &mut PlotWorld, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.state {
            Block::StonePressurePlate { .. } => {
                self.set_node(
                    node_id,
                    Block::StonePressurePlate { powered },
                    bool_to_ss(powered),
                );
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.state),
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

            match node.state {
                Block::RedstoneRepeater { mut repeater } => {
                    if repeater.locked {
                        continue;
                    }

                    let should_be_powered = input_power > 0;
                    if repeater.powered && !should_be_powered {
                        repeater.powered = false;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, 0);
                    } else if !repeater.powered {
                        repeater.powered = true;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, 15);
                    }
                }
                Block::RedstoneTorch { lit } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: false }, 0);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: true }, 15);
                    }
                }
                Block::RedstoneWallTorch { lit, facing } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(node_id, Block::RedstoneWallTorch { lit: false, facing }, 0);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, Block::RedstoneWallTorch { lit: true, facing }, 15);
                    }
                }
                Block::RedstoneComparator { mut comparator } => {
                    if let Some(far_override) = node.comparator_far_input {
                        if input_power < 15 {
                            input_power = far_override;
                        }
                    }
                    let new_strength =
                        calculate_comparator_output(comparator.mode, input_power, side_input_power);
                    let old_strength = node.output_power;
                    if new_strength != old_strength || comparator.mode == ComparatorMode::Compare {
                        let should_be_powered = comparator_should_be_powered(
                            comparator.mode,
                            input_power,
                            side_input_power,
                        );
                        let powered = comparator.powered;
                        if powered && !should_be_powered {
                            comparator.powered = false;
                        } else if !powered && should_be_powered {
                            comparator.powered = true;
                        }
                        self.set_node(
                            node_id,
                            Block::RedstoneComparator { comparator },
                            new_strength,
                        );
                    }
                }
                Block::RedstoneLamp { lit } => {
                    let should_be_lit = input_power > 0;
                    if lit && !should_be_lit {
                        self.set_node(node_id, Block::RedstoneLamp { lit: false }, 0);
                    }
                }
                Block::StoneButton { mut button } => {
                    if button.powered {
                        button.powered = false;
                        self.set_node(node_id, Block::StoneButton { button }, 0);
                    }
                }
                _ => warn!("Node {:?} should not be ticked!", node.state),
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
            if node.changed && (!io_only || is_io_block(node.state)) {
                plot.set_block(node.pos, node.state);
            }
            node.changed = false;
        }
    }
}

fn set_node(node: &mut Node, new_state: Block) {
    node.state = new_state;
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

    let node = &mut nodes[node_id];
    match node.state {
        Block::RedstoneRepeater { mut repeater } => {
            let should_be_locked = side_input_power > 0;
            if !repeater.locked && should_be_locked {
                repeater.locked = true;
                set_node(node, Block::RedstoneRepeater { repeater });
            } else if repeater.locked && !should_be_locked {
                repeater.locked = false;
                set_node(node, Block::RedstoneRepeater { repeater });
            }

            if !repeater.locked && !node.pending_tick {
                let should_be_powered = input_power > 0;
                if should_be_powered != repeater.powered {
                    let priority = if node.facing_diode {
                        TickPriority::Highest
                    } else if !should_be_powered {
                        TickPriority::Higher
                    } else {
                        TickPriority::High
                    };
                    schedule_tick(scheduler, node_id, node, repeater.delay as usize, priority);
                }
            }
        }
        Block::RedstoneTorch { lit } | Block::RedstoneWallTorch { lit, .. } => {
            if lit == (input_power > 0) && !node.pending_tick {
                schedule_tick(scheduler, node_id, node, 1, TickPriority::Normal);
            }
        }
        Block::RedstoneComparator { comparator } => {
            if node.pending_tick {
                return;
            }
            if let Some(far_override) = node.comparator_far_input {
                if input_power < 15 {
                    input_power = far_override;
                }
            }
            let output_power =
                calculate_comparator_output(comparator.mode, input_power, side_input_power);
            let old_strength = node.output_power;
            if output_power != old_strength
                || comparator.powered
                    != comparator_should_be_powered(comparator.mode, input_power, side_input_power)
            {
                let priority = if node.facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(scheduler, node_id, node, 1, priority);
            }
        }
        Block::RedstoneLamp { lit } => {
            let should_be_lit = input_power > 0;
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node(node, Block::RedstoneLamp { lit: true });
            }
        }
        Block::IronTrapdoor {
            facing,
            half,
            powered,
        } => {
            let should_be_powered = input_power > 0;
            if powered != should_be_powered {
                let new_block = Block::IronTrapdoor {
                    facing,
                    half,
                    powered: should_be_powered,
                };
                set_node(node, new_block);
            }
        }
        Block::RedstoneWire { mut wire } => {
            if wire.power != input_power {
                wire.power = input_power;
                set_node(node, Block::RedstoneWire { wire });
            }
        }
        _ => {} // panic!("Node {:?} should not be updated!", node.state),
    }
}

impl fmt::Display for DirectBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("digraph{")?;
        for (id, node) in self.nodes.inner().iter().enumerate() {
            if matches!(node.state, Block::RedstoneWire { .. }) {
                continue;
            }
            write!(
                f,
                "n{}[label=\"{}\\n({}, {}, {})\"];",
                id,
                format!("{:?}", node.state)
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
