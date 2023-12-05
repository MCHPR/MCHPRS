//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx};
use crate::redpiler::{block_powered_mut, bool_to_ss};
use crate::world::World;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::blocks::{Block, ComparatorMode};
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, TickPriority};
use nodes::{NodeId, Nodes};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{fmt, mem};
use tracing::{debug, trace, warn};

#[derive(Debug, Default)]
struct FinalGraphStats {
    update_link_count: usize,
    side_link_count: usize,
    default_link_count: usize,
    nodes_bytes: usize,
}

mod nodes {
    use super::Node;
    use std::ops::{Index, IndexMut};

    #[derive(Debug, Copy, Clone)]
    pub struct NodeId(u32);

    impl NodeId {
        pub fn index(self) -> usize {
            self.0 as usize
        }

        /// Safety: index must be within bounds of nodes array
        pub unsafe fn from_index(index: usize) -> NodeId {
            NodeId(index as u32)
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
                NodeId(idx as u32)
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
            unsafe { self.nodes.get_unchecked(index.0 as usize) }
        }
    }

    impl IndexMut<NodeId> for Nodes {
        fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
            unsafe { self.nodes.get_unchecked_mut(index.0 as usize) }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ForwardLink {
    data: u32
}
impl ForwardLink {
    pub fn new(id: NodeId, side: bool, mut ss: u8) -> Self {
        assert!(id.index() < (1 << 27));
        if ss >= 16 {
            ss = 15;
        }
        Self { data:  (id.index() as u32) << 5 | if side {1 << 4} else {0} | ss as u32}
    }
    pub fn node(self) -> NodeId {
        unsafe {
            // safety: ForwardLink is constructed using a NodeId
            NodeId::from_index((self.data >> 5) as usize)
        }
    }
    pub fn side(self) -> bool {
        self.data & (1 << 4) != 0
    }
    pub fn ss(self) -> u8 {
        (self.data & 0b1111) as u8
    }
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

// struct is 128 bytes to fit nicely into cachelines
// which are usualy 64 bytes, it can vary but is almost always a power of 2
#[derive(Debug, Clone)]
#[repr(align(128))]
pub struct Node {
    ty: NodeType,
    default_inputs: [u8; 16],
    side_inputs: [u8; 16],
    updates: SmallVec<[ForwardLink; 18]>,
    
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
    fn from_compile_node(
        graph: &CompileGraph,
        node_idx: NodeIdx,
        nodes_len: usize,
        nodes_map: &FxHashMap<NodeIdx, usize>,
        stats: &mut FinalGraphStats,
    ) -> Self {
        let node = &graph[node_idx];

        let mut default_inputs = [0; 16];
        let mut side_inputs = [0; 16];
        for edge in graph.edges_directed(node_idx, Direction::Incoming) {
            let weight = edge.weight();
            let distance = weight.ss;
            let source = edge.source();
            let ss = graph[source].state.output_strength.saturating_sub(distance);
            match weight.ty {
                LinkType::Default => default_inputs[ss as usize] += 1,
                LinkType::Side => side_inputs[ss as usize] += 1,
            }
        }
        stats.default_link_count += default_inputs.len();
        stats.side_link_count += side_inputs.len();

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
                if side_inputs.is_empty() {
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
    queues_deque: [Queues; Self::NUM_QUEUES],
    pos: usize,
}

impl TickScheduler {
    const NUM_PRIORITIES: usize = 4;
    const NUM_QUEUES: usize = 16;

    fn reset<W: World>(&mut self, world: &mut W, blocks: &[Option<(BlockPos, Block)>]) {
        for (idx, queues) in self.queues_deque.iter().enumerate() {
            let delay = if self.pos >= idx { idx + Self::NUM_QUEUES } else { idx } - self.pos;
            for (entries, priority) in queues.0.iter().zip(Self::priorities()) {
                for node in entries {
                    let Some((pos, _)) = blocks[node.index()] else {
                        warn!("Cannot schedule tick for node {:?} because block information is missing", node);
                        continue;
                    };
                    world.schedule_tick(pos, delay as u32, priority);
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
        self.queues_deque[(self.pos + delay) % Self::NUM_QUEUES].0[Self::priority_index(priority)].push(node);
    }

    fn queues_this_tick(&mut self) -> Queues {
        self.pos = (self.pos + 1) % Self::NUM_QUEUES;
        mem::take(&mut self.queues_deque[self.pos])
    }

    fn end_tick(&mut self, mut queues: Queues) {
        for queue in &mut queues.0 {
            queue.clear();
        }
        self.queues_deque[self.pos] = queues;
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
    blocks: Vec<Option<(BlockPos, Block)>>,
    pos_map: FxHashMap<BlockPos, NodeId>,
    scheduler: TickScheduler,
}

impl DirectBackend {
    fn schedule_tick(&mut self, node_id: NodeId, delay: usize, priority: TickPriority) {
        self.scheduler.schedule_tick(node_id, delay, priority);
    }

    fn set_node(&mut self, node_id: NodeId, powered: bool, new_power: u8) {
        let node = &mut self.nodes[node_id];
        let old_power = node.output_power;

        node.changed = true;
        node.powered = powered;
        node.output_power = new_power;
        for i in 0..node.updates.len() {
            let node = &self.nodes[node_id];
            let update_link = node.updates[i];
            let side = update_link.side();
            let distance = update_link.ss();
            let update = update_link.node();

            let update_ref = &mut self.nodes[update];
            let inputs = if side {
                &mut update_ref.side_inputs
            } else {
                &mut update_ref.default_inputs
            };
            inputs[old_power.saturating_sub(distance) as usize] -= 1;
            inputs[new_power.saturating_sub(distance) as usize] += 1;

            update_node(&mut self.scheduler, &mut self.nodes, update);
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

    fn reset<W: World>(&mut self, world: &mut W, io_only: bool) {
        self.scheduler.reset(world, &self.blocks);

        let nodes = std::mem::take(&mut self.nodes);

        for (i, node) in nodes.into_inner().iter().enumerate() {
            let Some((pos, block)) = self.blocks[i] else {
                continue;
            };
            if matches!(node.ty, NodeType::Comparator(_)) {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_power,
                };
                world.set_block_entity(pos, block_entity);
            }

            if io_only && !node.ty.is_io_block() {
                world.set_block(pos, block);
            }
        }

        self.pos_map.clear();
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
        let mut queues = self.scheduler.queues_this_tick();

        for node_id in queues.drain_iter() {
            self.nodes[node_id].pending_tick = false;
            let node = &self.nodes[node_id];

            match node.ty {
                NodeType::Repeater(delay) => {
                    if node.locked {
                        continue;
                    }

                    let should_be_powered = get_bool_input(node);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                        if !should_be_powered {
                            let node = &mut self.nodes[node_id];
                            schedule_tick(
                                &mut self.scheduler,
                                node_id,
                                node,
                                delay as usize,
                                TickPriority::Higher,
                            );
                        }
                    }
                }
                NodeType::SimpleRepeater(delay) => {
                    let should_be_powered = get_bool_input(node);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                        if !should_be_powered {
                            let node = &mut self.nodes[node_id];
                            schedule_tick(
                                &mut self.scheduler,
                                node_id,
                                node,
                                delay as usize,
                                TickPriority::Higher,
                            );
                        }
                    }
                }
                NodeType::Torch => {
                    let should_be_off = get_bool_input(node);
                    let lit = node.powered;
                    if lit && should_be_off {
                        self.set_node(node_id, false, 0);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, true, 15);
                    }
                }
                NodeType::Comparator(mode) => {
                    let (mut input_power, side_input_power) = get_all_input(node);
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
                    let should_be_lit = get_bool_input(node);
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

    fn compile(&mut self, graph: CompileGraph, ticks: Vec<TickEntry>) {
        let mut nodes_map =
            FxHashMap::with_capacity_and_hasher(graph.node_count(), Default::default());
        for node in graph.node_indices() {
            nodes_map.insert(node, nodes_map.len());
        }
        let nodes_len = nodes_map.len();

        let mut stats = FinalGraphStats::default();
        let nodes = graph
            .node_indices()
            .map(|idx| Node::from_compile_node(&graph, idx, nodes_len, &nodes_map, &mut stats))
            .collect();
        stats.nodes_bytes = nodes_len * std::mem::size_of::<Node>();
        trace!("{:#?}", stats);

        self.blocks = graph
            .node_weights()
            .map(|node| node.block.map(|(pos, id)| (pos, Block::from_id(id))))
            .collect();
        self.nodes = Nodes::new(nodes);

        for i in 0..self.blocks.len() {
            if let Some((pos, _)) = self.blocks[i] {
                self.pos_map.insert(pos, self.nodes.get(i));
            }
        }

        for entry in ticks {
            if let Some(node) = self.pos_map.get(&entry.pos) {
                self.scheduler
                    .schedule_tick(*node, entry.ticks_left as usize, entry.tick_priority);
                self.nodes[*node].pending_tick = true;
            }
        }
        // Dot file output
        // println!("{}", self);
    }

    fn flush<W: World>(&mut self, world: &mut W, io_only: bool) {
        for (i, node) in self.nodes.inner_mut().iter_mut().enumerate() {
            let Some((pos, block)) = &mut self.blocks[i] else {
                continue;
            };
            if node.changed && (!io_only || node.ty.is_io_block()) {
                if let Some(powered) = block_powered_mut(block) {
                    *powered = node.powered
                }
                if let Block::RedstoneWire { wire, .. } = block {
                    wire.power = node.output_power
                };
                if let Block::RedstoneRepeater { repeater } = block {
                    repeater.locked = node.locked;
                }
                world.set_block(*pos, *block);
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

const INPUT_MASK: u128 = u128::from_ne_bytes([0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255]);

fn get_bool_input(node: &Node) -> bool {
    u128::from_ne_bytes(node.default_inputs) & INPUT_MASK != 0
}

fn get_bool_side(node: &Node) -> bool {
    u128::from_ne_bytes(node.side_inputs) & INPUT_MASK != 0
}

fn last_index_positive(array: &[u8; 16]) -> u32 {
    // Note: this might be slower on big-endian systems
    let value = u128::from_le_bytes(*array);
    if value == 0 {0} else {15 - (value.leading_zeros() >> 3)}
}

fn get_all_input(node: &Node) -> (u8, u8) {
    let input_power = last_index_positive(&node.default_inputs) as u8;

    let side_input_power = last_index_positive(&node.side_inputs) as u8;

    (input_power, side_input_power)
}

#[inline(always)]
fn update_node(scheduler: &mut TickScheduler, nodes: &mut Nodes, node_id: NodeId) {
    let node = &nodes[node_id];

    match node.ty {
        NodeType::Repeater(delay) => {
            let node = &mut nodes[node_id];
            let should_be_locked = get_bool_side(node);
            if !node.locked && should_be_locked {
                set_node_locked(node, true);
            } else if node.locked && !should_be_locked {
                set_node_locked(node, false);
            }

            if !node.locked && !node.pending_tick {
                let should_be_powered = get_bool_input(node);
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
            let should_be_powered = get_bool_input(node);
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
            let should_be_off = get_bool_input(node);
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
            let (mut input_power, side_input_power) = get_all_input(node);
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
            let should_be_lit = get_bool_input(node);
            let lit = node.powered;
            let node = &mut nodes[node_id];
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node(node, true);
            }
        }
        NodeType::Trapdoor => {
            let should_be_powered = get_bool_input(node);
            if node.powered != should_be_powered {
                let node = &mut nodes[node_id];
                set_node(node, should_be_powered);
            }
        }
        NodeType::Wire => {
            let (input_power, _) = get_all_input(node);
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
            let label = match node.ty {
                NodeType::Constant => format!("Constant: {}", node.output_power),
                _ => format!("{:?}", node.ty)
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .to_string(),
            };
            let pos = if let Some((pos, _)) = self.blocks[id] {
                format!("{}, {}, {}", pos.x, pos.y, pos.z)
            } else {
                "No Pos".to_string()
            };
            write!(f, "n{}[label=\"{}\\n({})\"];", id, label, pos,)?;
            for link in node.updates.iter() {
                let out_index = link.node().index();
                let distance = link.ss();
                let color = if link.side() {",color=\"blue\""} else {""}; 
                write!(
                    f,
                    "n{}->n{}[label=\"{}\"{}];",
                    id,
                    out_index,
                    distance,
                    color
                )?;
            }
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
