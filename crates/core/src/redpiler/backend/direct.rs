//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::blocks::{Block, ComparatorMode, RedstoneRepeater};
use crate::plot::PlotWorld;
use crate::redpiler::{block_powered_mut, bool_to_ss, CompileNode, LinkType};
use crate::world::World;
use log::warn;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, TickPriority};
use nodes::{NodeId, Nodes};
use smallvec::SmallVec;
use std::collections::{HashMap, VecDeque};
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
}

#[derive(Debug, Clone, Copy)]
enum NodeType {
    Repeater(u8),
    /// A non-locking repeater
    SimpleRepeater(u8),
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
        matches!(
            self,
            NodeType::Lamp
                | NodeType::Button
                | NodeType::Lever
                | NodeType::Trapdoor
                | NodeType::PressurePlate
        )
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    ty: NodeType,
    default_inputs: SmallVec<[DirectLink; 2]>,
    side_inputs: SmallVec<[DirectLink; 1]>,
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
        let mut default_inputs = SmallVec::new();
        let mut side_inputs = SmallVec::new();
        node.inputs
            .into_iter()
            .map(|link| {
                assert!(link.end < nodes_len);
                (
                    link.ty,
                    DirectLink {
                        weight: link.weight,
                        to: unsafe {
                            // Safety: bounds checked
                            NodeId::from_index(link.end)
                        },
                    },
                )
            })
            .for_each(|(link_type, link)| match link_type {
                LinkType::Default => default_inputs.push(link),
                LinkType::Side => side_inputs.push(link),
            });
        let updates = node
            .updates
            .into_iter()
            .map(|idx| {
                assert!(idx < nodes_len);
                // Safety: bounds checked
                unsafe { NodeId::from_index(idx) }
            })
            .collect();
        let ty = match node.state {
            Block::RedstoneRepeater {
                repeater: RedstoneRepeater { delay, .. },
            } if side_inputs.is_empty() => NodeType::SimpleRepeater(delay),
            state => NodeType::new(state),
        };
        Node {
            ty,
            default_inputs,
            side_inputs,
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

#[derive(Default, Clone)]
struct Queues([Vec<NodeId>; TickScheduler::NUM_PRIORITIES]);

impl Queues {
    fn drain_iter(&mut self) -> impl Iterator<Item = NodeId> + '_ {
        let [q0, q1, q2, q3] = &mut self.0;
        let [q0, q1, q2, q3] = [q0, q1, q2, q3].map(|q| q.drain(..));
        q0.chain(q1).chain(q2).chain(q3)
    }
}

#[derive(Default)]
struct TickScheduler {
    queues_deque: VecDeque<Queues>,
}

impl TickScheduler {
    const NUM_PRIORITIES: usize = 4;

    fn reset(&mut self, plot: &mut PlotWorld, blocks: &[(BlockPos, Block)]) {
        for (delay, queues) in self.queues_deque.iter().enumerate() {
            for (entries, priority) in queues.0.iter().zip(Self::priorities()) {
                for node in entries {
                    let pos = blocks[node.index()].0;
                    plot.schedule_tick(pos, delay as u32, priority);
                }
            }
        }
        self.queues_deque.clear();
    }

    fn schedule_tick(&mut self, node: NodeId, delay: usize, priority: TickPriority) {
        if delay >= self.queues_deque.len() {
            self.queues_deque.resize(delay + 1, Default::default());
        }

        self.queues_deque[delay].0[Self::priority_index(priority)].push(node);
    }

    fn queues_this_tick(&mut self) -> Queues {
        if self.queues_deque.len() == 0 {
            self.queues_deque.push_back(Default::default());
        }
        let queues = self.queues_deque.pop_front().unwrap();
        self.queues_deque.push_front(Default::default());
        queues
    }

    fn end_tick(&mut self, mut queues: Queues) {
        self.queues_deque.pop_front();

        for queue in &mut queues.0 {
            queue.clear();
        }
        self.queues_deque.push_back(queues);
    }

    fn priorities() -> [TickPriority; Self::NUM_PRIORITIES] {
        [
            TickPriority::Highest,
            TickPriority::Higher,
            TickPriority::High,
            TickPriority::Normal,
        ]
    }

    fn priority_index(priority: TickPriority) -> usize {
        match priority {
            TickPriority::Highest => 0,
            TickPriority::Higher => 1,
            TickPriority::High => 2,
            TickPriority::Normal => 3,
        }
    }
}

#[derive(Default)]
pub struct DirectBackend {
    nodes: Nodes,
    blocks: Vec<(BlockPos, Block)>,
    pos_map: HashMap<BlockPos, NodeId>,
    scheduler: TickScheduler,
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
        self.scheduler.reset(plot, &self.blocks);

        let nodes = std::mem::take(&mut self.nodes);

        for (i, node) in nodes.into_inner().iter().enumerate() {
            let (pos, block) = self.blocks[i];
            if matches!(node.ty, NodeType::Comparator(_)) {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_power,
                };
                plot.set_block_entity(pos, block_entity);
            }

            if io_only && !node.ty.is_io_block() {
                plot.set_block(pos, block);
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
                self.set_node(node_id, powered, bool_to_ss(powered));
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
                self.set_node(node_id, powered, bool_to_ss(powered));
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
        }
    }

    fn tick(&mut self, _plot: &mut PlotWorld) {
        let mut queues = self.scheduler.queues_this_tick();

        for node_id in queues.drain_iter() {
            self.nodes[node_id].pending_tick = false;
            let node = &self.nodes[node_id];

            match node.ty {
                NodeType::Repeater(_) => {
                    if node.locked {
                        continue;
                    }

                    let should_be_powered = get_bool_input(node, &self.nodes);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::SimpleRepeater(_delay) => {
                    let should_be_powered = get_bool_input(node, &self.nodes);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::Torch => {
                    let should_be_off = get_bool_input(node, &self.nodes);
                    let lit = node.powered;
                    if lit && should_be_off {
                        self.set_node(node_id, false, 0);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::Comparator(mode) => {
                    let (mut input_power, side_input_power) = get_all_input(node, &self.nodes);
                    if let Some(far_override) = node.comparator_far_input {
                        if input_power < 15 {
                            input_power = far_override;
                        }
                    }
                    let old_strength = node.output_power;
                    let new_strength =
                        calculate_comparator_output(mode, input_power, side_input_power);
                    if new_strength != old_strength {
                        self.set_node(node_id, new_strength > 0, new_strength);
                    }
                }
                NodeType::Lamp => {
                    let should_be_lit = get_bool_input(node, &self.nodes);
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

        self.scheduler.end_tick(queues);
    }

    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {
        let nodes_len = nodes.len();
        self.blocks = nodes.iter().map(|node| (node.pos, node.state)).collect();
        let nodes = nodes
            .into_iter()
            .map(|cn| Node::from_compile_node(cn, nodes_len))
            .collect();
        self.nodes = Nodes::new(nodes);
        for i in 0..self.blocks.len() {
            self.pos_map.insert(self.blocks[i].0, self.nodes.get(i));
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
        for (i, node) in self.nodes.inner_mut().iter_mut().enumerate() {
            let (pos, block) = &mut self.blocks[i];
            if node.changed && (!io_only || node.ty.is_io_block()) {
                if let Some(powered) = block_powered_mut(block) {
                    *powered = node.powered
                }
                if let Block::RedstoneWire { wire, .. } = block {
                    wire.power = node.output_power
                };
                plot.set_block(*pos, *block);
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

fn link_strength(link: &DirectLink, nodes: &Nodes) -> u8 {
    nodes[link.to].output_power.saturating_sub(link.weight)
}

fn get_bool_input(node: &Node, nodes: &Nodes) -> bool {
    node.default_inputs
        .iter()
        .any(|link| link_strength(link, nodes) > 0)
}

fn get_all_input(node: &Node, nodes: &Nodes) -> (u8, u8) {
    let input_power = node
        .default_inputs
        .iter()
        .map(|link| link_strength(link, nodes))
        .max()
        .unwrap_or(0);

    let side_input_power = node
        .side_inputs
        .iter()
        .map(|link| link_strength(link, nodes))
        .max()
        .unwrap_or(0);

    (input_power, side_input_power)
}

fn update_node(scheduler: &mut TickScheduler, nodes: &mut Nodes, node_id: NodeId) {
    let node = &nodes[node_id];

    match node.ty {
        NodeType::Repeater(delay) => {
            let (input_power, side_input_power) = get_all_input(node, nodes);
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
        NodeType::SimpleRepeater(delay) => {
            if node.pending_tick {
                return;
            }
            let should_be_powered = get_bool_input(node, nodes);
            if node.powered != should_be_powered {
                let priority = if node.facing_diode {
                    TickPriority::Highest
                } else if !should_be_powered {
                    TickPriority::Higher
                } else {
                    TickPriority::High
                };
                let node = &mut nodes[node_id];
                schedule_tick(scheduler, node_id, node, delay as usize, priority);
            }
        }
        NodeType::Torch => {
            if node.pending_tick {
                return;
            }
            let should_be_off = get_bool_input(node, nodes);
            let lit = node.powered;
            if lit == should_be_off {
                let node = &mut nodes[node_id];
                schedule_tick(scheduler, node_id, node, 1, TickPriority::Normal);
            }
        }
        NodeType::Comparator(mode) => {
            if node.pending_tick {
                return;
            }
            let (mut input_power, side_input_power) = get_all_input(node, nodes);
            if let Some(far_override) = node.comparator_far_input {
                if input_power < 15 {
                    input_power = far_override;
                }
            }
            let old_strength = node.output_power;
            let output_power = calculate_comparator_output(mode, input_power, side_input_power);
            if output_power != old_strength {
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
            let should_be_lit = get_bool_input(node, nodes);
            let lit = node.powered;
            let node = &mut nodes[node_id];
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node(node, true);
            }
        }
        NodeType::Trapdoor => {
            let should_be_powered = get_bool_input(node, nodes);
            if node.powered != should_be_powered {
                let node = &mut nodes[node_id];
                set_node(node, should_be_powered);
            }
        }
        NodeType::Wire => {
            let (input_power, _) = get_all_input(node, nodes);
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
            let pos = self.blocks[id].0;
            write!(
                f,
                "n{}[label=\"{}\\n({}, {}, {})\"];",
                id,
                format!("{:?}", node.ty).split_whitespace().next().unwrap(),
                pos.x,
                pos.y,
                pos.z
            )?;
            let all_inputs = node
                .default_inputs
                .iter()
                .map(|link| (LinkType::Default, link))
                .chain(node.side_inputs.iter().map(|link| (LinkType::Side, link)));
            for (link_type, link) in all_inputs {
                let color = match link_type {
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

fn calculate_comparator_output(mode: ComparatorMode, input_strength: u8, power_on_sides: u8) -> u8 {
    match mode {
        ComparatorMode::Compare => {
            if input_strength >= power_on_sides {
                input_strength
            } else {
                0
            }
        }
        ComparatorMode::Subtract => input_strength.saturating_sub(power_on_sides),
    }
}
