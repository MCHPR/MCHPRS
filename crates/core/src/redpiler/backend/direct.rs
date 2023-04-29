//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::blocks::{Block, ComparatorMode};
use crate::redpiler::compile_graph::{CompileGraph, LinkType, NodeIdx};
use crate::redpiler::{block_powered_mut, bool_to_ss};
use crate::world::World;
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, TickPriority};
use nodes::{NodeId, Nodes};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use smallvec::SmallVec;
use std::collections::HashMap;
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
    default_inputs: SmallVec<[DirectLink; 7]>,
    side_inputs: SmallVec<[DirectLink; 2]>,
    updates: SmallVec<[NodeId; 4]>,
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
        nodes_map: &HashMap<NodeIdx, usize>,
        stats: &mut FinalGraphStats,
    ) -> Self {
        let node = &graph[node_idx];

        let mut default_inputs = SmallVec::new();
        let mut side_inputs = SmallVec::new();
        for edge in graph.edges_directed(node_idx, Direction::Incoming) {
            let idx = nodes_map[&edge.source()];
            assert!(idx < nodes_len);
            let idx = unsafe {
                // Safety: bounds checked
                NodeId::from_index(idx)
            };
            let link = DirectLink {
                to: idx,
                weight: edge.weight().ss,
            };
            match edge.weight().ty {
                LinkType::Default => default_inputs.push(link),
                LinkType::Side => side_inputs.push(link),
            }
        }
        stats.default_link_count += default_inputs.len();
        stats.side_link_count += side_inputs.len();

        use crate::redpiler::compile_graph::NodeType as CNodeType;
        let updates: SmallVec<[NodeId; 4]> = if node.ty != CNodeType::Constant {
            graph
                .neighbors_directed(node_idx, Direction::Outgoing)
                .map(|idx| unsafe {
                    let idx = nodes_map[&idx];
                    assert!(idx < nodes_len);
                    // Safety: bounds checked
                    NodeId::from_index(idx)
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
    queues_deque: [Queues; 16],
    pos: usize,
}

impl TickScheduler {
    const NUM_PRIORITIES: usize = 4;

    fn reset<W: World>(&mut self, world: &mut W, blocks: &[Option<(BlockPos, Block)>]) {
        for (delay, queues) in self.queues_deque.iter().enumerate() {
            for (entries, priority) in queues.0.iter().zip(Self::priorities()) {
                for node in entries {
                    let Some((pos, _)) = blocks[node.index()] else {
                        warn!("Cannot schedule tick for node {:?} because block information is missing", node);
                        continue;
                    };
                    world.schedule_tick(pos, delay as u32 + 1, priority);
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
        self.queues_deque[(self.pos + delay) & 15].0[Self::priority_index(priority)].push(node);
    }

    fn queues_this_tick(&mut self) -> Queues {
        mem::take(&mut self.queues_deque[self.pos])
    }

    fn end_tick(&mut self, mut queues: Queues) {
        for queue in &mut queues.0 {
            queue.clear();
        }
        self.queues_deque[self.pos] = queues;

        self.pos = (self.pos + 1) & 15;
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
    }
}

impl<W: World> JITBackend<W> for DirectBackend {
    fn inspect(&mut self, pos: BlockPos) {
        let Some(node_id) = self.pos_map.get(&pos) else {
            debug!("could not find node at pos {}", pos);
            return;
        };

        debug!("Node {:?}: {:#?}", node_id, self.nodes[*node_id]);
    }

    fn reset(&mut self, world: &mut W, io_only: bool) {
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

    fn on_use_block(&mut self, _world: &mut W, pos: BlockPos) {
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

    fn set_pressure_plate(&mut self, _world: &mut W, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::PressurePlate => {
                self.set_node(node_id, powered, bool_to_ss(powered));
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
        }
    }

    fn tick(&mut self, _world: &mut W) {
        let mut queues = self.scheduler.queues_this_tick();

        for node_id in queues.drain_iter() {
            self.nodes[node_id].pending_tick = false;
            let node = &self.nodes[node_id];

            match node.ty {
                NodeType::Repeater(delay) => {
                    if node.locked {
                        continue;
                    }

                    let should_be_powered = get_bool_input(node, &self.nodes);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                        if !should_be_powered {
                            let node = &mut self.nodes[node_id];
                            schedule_tick(&mut self.scheduler, node_id, node, delay as usize, TickPriority::Higher);
                        }
                    }
                }
                NodeType::SimpleRepeater(delay) => {
                    let should_be_powered = get_bool_input(node, &self.nodes);
                    if node.powered && !should_be_powered {
                        self.set_node(node_id, false, 0);
                    } else if !node.powered {
                        self.set_node(node_id, true, 15);
                        if !should_be_powered {
                            let node = &mut self.nodes[node_id];
                            schedule_tick(&mut self.scheduler, node_id, node, delay as usize, TickPriority::Higher);
                        }
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

    fn compile(&mut self, graph: CompileGraph, ticks: Vec<TickEntry>) {
        let mut nodes_map = HashMap::with_capacity(graph.node_count());
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

        let queues = self.scheduler.queues_this_tick();
        for entry in ticks {
            if let Some(node) = self.pos_map.get(&entry.pos) {
                self.scheduler
                    .schedule_tick(*node, entry.ticks_left as usize, entry.tick_priority);
            }
        }
        self.scheduler.end_tick(queues);
        // Dot file output
        // println!("{}", self);
    }

    fn flush(&mut self, world: &mut W, io_only: bool) {
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

fn link_strength(link: DirectLink, nodes: &Nodes) -> u8 {
    nodes[link.to].output_power.saturating_sub(link.weight)
}

fn get_bool_input(node: &Node, nodes: &Nodes) -> bool {
    node.default_inputs
        .iter()
        .copied()
        .any(|link| link_strength(link, nodes) > 0)
}

fn get_all_input(node: &Node, nodes: &Nodes) -> (u8, u8) {
    let input_power = node
        .default_inputs
        .iter()
        .copied()
        .map(|link| link_strength(link, nodes))
        .max()
        .unwrap_or(0);

    let side_input_power = node
        .side_inputs
        .iter()
        .copied()
        .map(|link| link_strength(link, nodes))
        .max()
        .unwrap_or(0);

    (input_power, side_input_power)
}

#[inline(always)]
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
