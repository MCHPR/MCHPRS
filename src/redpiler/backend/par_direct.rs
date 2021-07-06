use super::{JITBackend, JITResetData};
use crate::blocks::{Block, BlockEntity, BlockPos, ComparatorMode};
use crate::redpiler::{Link, LinkType, Node};
use crate::world::{TickEntry, TickPriority};
use log::warn;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Debug)]
struct RPTickEntry {
    ticks_left: i32,
    tick_priority: TickPriority,
    node: usize,
}

#[derive(Debug)]
enum PNodeType {
    Repeater(u8),
    Comparator(ComparatorMode),
    Torch,
    WallTorch,
    Wire,
    StoneButton,
    Lamp,
    RedstoneBlock,
    Container,
    Lever,
}

enum BlockChange {
    Power(usize, u8),
    RepeaterLock(usize, bool),
}

struct PNode {
    // Constant
    ty: PNodeType,
    facing_diode: bool,
    inputs: Vec<Link>,
    outputs: Vec<usize>,
    // State
    update_queued: AtomicBool,
    powered: AtomicBool,
    /// Only used for comparators and containers
    output_strength: AtomicU8,
    /// Repeater locking
    locked: AtomicBool,
    pending_tick: AtomicBool,
}

impl From<Node> for PNode {
    fn from(node: Node) -> Self {
        PNode {
            ty: match node.state {
                Block::RedstoneRepeater { repeater } => PNodeType::Repeater(repeater.delay),
                Block::RedstoneComparator { comparator } => PNodeType::Comparator(comparator.mode),
                Block::RedstoneTorch { .. } => PNodeType::Torch,
                Block::RedstoneWallTorch { .. } => PNodeType::WallTorch,
                Block::RedstoneWire { .. } => PNodeType::Wire,
                Block::StoneButton { .. } => PNodeType::StoneButton,
                Block::RedstoneLamp { .. } => PNodeType::Lamp,
                Block::RedstoneBlock { .. } => PNodeType::RedstoneBlock,
                Block::Lever { .. } => PNodeType::Lever,
                block if block.has_comparator_override() => PNodeType::Container,
                _ => unreachable!(),
            },
            facing_diode: node.facing_diode,
            inputs: node.inputs,
            outputs: node.updates.into_iter().map(|id| id.index).collect(),
            update_queued: AtomicBool::new(false),
            powered: AtomicBool::new(match node.state {
                Block::RedstoneRepeater { repeater } => repeater.powered,
                Block::RedstoneComparator { comparator } => comparator.powered,
                Block::RedstoneTorch { lit } => lit,
                Block::RedstoneWallTorch { lit, .. } => lit,
                Block::StoneButton { button } => button.powered,
                Block::RedstoneLamp { lit } => lit,
                Block::Lever { lever } => lever.powered,
                Block::RedstoneBlock {} => true,
                Block::RedstoneWire { .. } => false,
                _ => unreachable!(),
            }),
            output_strength: AtomicU8::new(node.comparator_output),
            locked: AtomicBool::new(if let Block::RedstoneRepeater { repeater } = node.state {
                repeater.locked
            } else {
                false
            }),
            pending_tick: AtomicBool::new(false),
        }
    }
}

impl PNode {
    fn get_output_power(&self) -> u8 {
        match self.ty {
            PNodeType::Comparator(_) => self.output_strength.load(Ordering::Relaxed),
            _ => {
                if self.powered.load(Ordering::Relaxed) {
                    15
                } else {
                    0
                }
            }
        }
    }
}
pub struct ParDirectBackend {
    blocks: Vec<(BlockPos, Block)>,
    block_changes: Vec<(BlockPos, Block)>,
    nodes: Arc<Vec<PNode>>,
    updates_tx: Sender<usize>,
    updates_rx: Receiver<usize>,
    ticks_tx: Sender<RPTickEntry>,
    ticks_rx: Receiver<RPTickEntry>,
    changes_tx: Sender<BlockChange>,
    changes_rx: Receiver<BlockChange>,
    updates: Vec<usize>,
    ticks: Vec<RPTickEntry>,
    pos_map: HashMap<BlockPos, usize>,
}

impl Default for ParDirectBackend {
    fn default() -> Self {
        let (updates_tx, updates_rx) = channel();
        let (ticks_tx, ticks_rx) = channel();
        let (changes_tx, changes_rx) = channel();
        Self {
            blocks: Vec::new(),
            nodes: Arc::new(vec![]),
            block_changes: vec![],
            updates_tx,
            updates_rx,
            ticks_tx,
            ticks_rx,
            changes_tx,
            changes_rx,
            updates: vec![],
            ticks: vec![],
            pos_map: HashMap::new(),
        }
    }
}

impl JITBackend for ParDirectBackend {
    fn reset(&mut self) -> JITResetData {
        let mut ticks = Vec::new();
        self.ticks.retain(|tick| tick.ticks_left >= 0);
        for entry in self.ticks.drain(..) {
            ticks.push(TickEntry {
                ticks_left: entry.ticks_left as u32,
                tick_priority: entry.tick_priority,
                pos: self.blocks[entry.node].0,
            })
        }

        let mut block_entities = Vec::new();
        for (node_id, node) in self.nodes.iter().enumerate() {
            if let PNodeType::Comparator(_) = node.ty {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_strength.load(Ordering::Relaxed),
                };
                block_entities.push((self.blocks[node_id].0, block_entity));
            }
        }

        // TODO: Reuse the old allocations
        *self = Default::default();
        Default::default()
    }

    fn on_use_block(&mut self, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            PNodeType::Lever => {
                let powered = !node.powered.load(Ordering::Relaxed);
                node.powered.store(powered, Ordering::Relaxed);
                self.changes_tx
                    .send(BlockChange::Power(node_id, powered as u8))
                    .unwrap();
                schedule_updates(&self.updates_tx, &self.nodes, node_id);
            }
            PNodeType::StoneButton => {
                let powered = !node.powered.load(Ordering::Relaxed);
                node.powered.store(powered, Ordering::Relaxed);
                self.changes_tx
                    .send(BlockChange::Power(node_id, powered as u8))
                    .unwrap();
                schedule_tick(&self.ticks_tx, node_id, 10, TickPriority::Normal);
                schedule_updates(&self.updates_tx, &self.nodes, node_id);
            }
            _ => {}
        }
        self.run_updates();
        self.collect_changes();
    }

    fn tick(&mut self) {
        // TODO: Tick priorities
        self.ticks.extend(self.ticks_rx.try_iter());
        self.ticks.par_iter_mut().for_each_with(
            (
                self.updates_tx.clone(),
                self.changes_tx.clone(),
                self.nodes.clone(),
            ),
            |(updates_tx, changes_tx, nodes), tick: &mut RPTickEntry| {
                if tick.ticks_left == 0 {
                    tick_single(tick.node, nodes, updates_tx, changes_tx)
                }
                tick.ticks_left -= 1;
            },
        );
        self.ticks.retain(|tick| tick.ticks_left >= 0);

        self.run_updates();
        self.collect_changes();
    }

    fn compile(&mut self, nodes: Vec<Node>, ticks: Vec<TickEntry>) {
        for (i, node) in nodes.iter().enumerate() {
            self.blocks.push((node.pos, node.state));
            self.pos_map.insert(node.pos, i);
        }
        for entry in ticks {
            if let Some(node) = self.pos_map.get(&entry.pos) {
                self.ticks.push(RPTickEntry {
                    ticks_left: entry.ticks_left as i32,
                    tick_priority: entry.tick_priority,
                    node: *node,
                });
            }
        }
        let pnodes = nodes.into_iter().map(Into::into).collect();
        self.nodes = Arc::new(pnodes);
    }

    fn block_changes(&mut self) -> &mut Vec<(BlockPos, Block)> {
        &mut self.block_changes
    }
}

impl ParDirectBackend {
    fn run_updates(&mut self) {
        self.updates.clear();
        self.updates.extend(self.updates_rx.try_iter());
        self.updates.par_iter().for_each_with(
            (
                self.ticks_tx.clone(),
                self.changes_tx.clone(),
                self.nodes.clone(),
            ),
            |(ticks_tx, changes_tx, nodes), node_id| {
                update_single(*node_id, nodes, ticks_tx, changes_tx)
            },
        );
    }

    fn collect_changes(&mut self) {
        for change in self.changes_rx.try_iter() {
            match change {
                BlockChange::Power(node_id, power) => {
                    let powered = power > 0;
                    match &mut self.blocks[node_id].1 {
                        Block::RedstoneComparator { comparator } => comparator.powered = powered,
                        Block::RedstoneTorch { lit } => *lit = powered,
                        Block::RedstoneWallTorch { lit, .. } => *lit = powered,
                        Block::RedstoneRepeater { repeater } => repeater.powered = powered,
                        Block::RedstoneWire { wire } => wire.power = power as u8,
                        Block::Lever { lever } => lever.powered = powered,
                        Block::StoneButton { button } => button.powered = powered,
                        Block::RedstoneLamp { lit } => *lit = powered,
                        _ => {}
                    }
                    self.block_changes.push(self.blocks[node_id]);
                }
                BlockChange::RepeaterLock(node_id, locked) => match &mut self.blocks[node_id].1 {
                    Block::RedstoneRepeater { repeater } => repeater.locked = locked,
                    _ => panic!("tried to lock a node which wasn't a repeater"),
                },
            }
        }
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

fn schedule_tick(
    ticks_tx: &Sender<RPTickEntry>,
    node_id: usize,
    delay: u32,
    priority: TickPriority,
) {
    ticks_tx
        .send(RPTickEntry {
            node: node_id,
            tick_priority: priority,
            ticks_left: delay as i32,
        })
        .unwrap()
}

fn schedule_updates(updates_tx: &Sender<usize>, nodes: &Arc<Vec<PNode>>, node_id: usize) {
    for link in &nodes[node_id].outputs {
        if !nodes[*link].update_queued.load(Ordering::Relaxed) {
            updates_tx.send(*link).unwrap();
        }
    }
}

fn update_single(
    node_id: usize,
    nodes: &Arc<Vec<PNode>>,
    ticks_tx: &Sender<RPTickEntry>,
    changes_tx: &Sender<BlockChange>,
) {
    let node = &nodes[node_id];
    node.update_queued.store(false, Ordering::Relaxed);

    let mut input_power = 0u8;
    let mut side_input_power = 0u8;
    for link in &node.inputs {
        let power = match link.ty {
            LinkType::Default => &mut input_power,
            LinkType::Side => &mut side_input_power,
        };
        *power = (*power).max(
            nodes[link.end.index]
                .get_output_power()
                .saturating_sub(link.weight),
        );
    }

    match node.ty {
        PNodeType::Repeater(delay) => {
            let should_be_locked = side_input_power > 0;
            let mut locked = node.locked.load(Ordering::Relaxed);
            if !locked && should_be_locked {
                locked = true;
                node.locked.store(true, Ordering::Relaxed);
                changes_tx
                    .send(BlockChange::RepeaterLock(node_id, true))
                    .unwrap();
            } else if locked && !should_be_locked {
                locked = false;
                node.locked.store(false, Ordering::Relaxed);
                changes_tx
                    .send(BlockChange::RepeaterLock(node_id, false))
                    .unwrap();
            }

            if !locked && !nodes[node_id].pending_tick.load(Ordering::Relaxed) {
                let powered = node.powered.load(Ordering::Relaxed);
                let should_be_powered = input_power > 0;
                if should_be_powered != powered {
                    let priority = if node.facing_diode {
                        TickPriority::Highest
                    } else if !should_be_powered {
                        TickPriority::Higher
                    } else {
                        TickPriority::High
                    };
                    schedule_tick(ticks_tx, node_id, delay as u32, priority);
                }
            }
        }
        PNodeType::Torch | PNodeType::WallTorch => {
            let lit = node.powered.load(Ordering::Relaxed);
            if lit == (input_power > 0) && !nodes[node_id].pending_tick.load(Ordering::Relaxed) {
                schedule_tick(ticks_tx, node_id, 1, TickPriority::Normal);
            }
        }
        PNodeType::Comparator(mode) => {
            if nodes[node_id].pending_tick.load(Ordering::Relaxed) {
                return;
            }
            let output_power = calculate_comparator_output(mode, input_power, side_input_power);
            let old_strength = node.output_strength.load(Ordering::Relaxed);
            let powered = node.powered.load(Ordering::Relaxed);
            if output_power != old_strength
                || powered != comparator_should_be_powered(mode, input_power, side_input_power)
            {
                let priority = if node.facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(ticks_tx, node_id, 1, priority);
            }
        }
        PNodeType::Lamp => {
            let should_be_lit = input_power > 0;
            let lit = node.powered.load(Ordering::Relaxed);
            if lit && !should_be_lit {
                schedule_tick(ticks_tx, node_id, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                node.powered.store(true, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 15)).unwrap();
            }
        }
        PNodeType::Wire => {
            let power = node.output_strength.load(Ordering::Relaxed);
            if power != input_power {
                node.output_strength.store(input_power, Ordering::Relaxed);
                changes_tx
                    .send(BlockChange::Power(node_id, input_power))
                    .unwrap();
            }
        }
        _ => {}
    }
}

fn tick_single(
    node_id: usize,
    nodes: &Arc<Vec<PNode>>,
    updates_tx: &Sender<usize>,
    changes_tx: &Sender<BlockChange>,
) {
    let node = &nodes[node_id];
    node.pending_tick.store(false, Ordering::Relaxed);

    let mut input_power = 0u8;
    let mut side_input_power = 0u8;
    for link in &node.inputs {
        let power = match link.ty {
            LinkType::Default => &mut input_power,
            LinkType::Side => &mut side_input_power,
        };
        *power = (*power).max(
            nodes[link.end.index]
                .get_output_power()
                .saturating_sub(link.weight),
        );
    }

    match node.ty {
        PNodeType::Repeater(_) => {
            if node.locked.load(Ordering::Relaxed) {
                return;
            }

            let should_be_powered = input_power > 0;
            let powered = node.powered.load(Ordering::Relaxed);
            if powered && !should_be_powered {
                node.powered.store(false, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 0)).unwrap();
                schedule_updates(updates_tx, nodes, node_id);
            } else if !powered {
                node.powered.store(true, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 15)).unwrap();
                schedule_updates(updates_tx, nodes, node_id);
            }
        }
        PNodeType::Torch | PNodeType::WallTorch => {
            let should_be_off = input_power > 0;
            let lit = node.powered.load(Ordering::Relaxed);
            if lit && should_be_off {
                node.powered.store(false, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 0)).unwrap();
                schedule_updates(updates_tx, nodes, node_id);
            } else if !lit && !should_be_off {
                node.powered.store(true, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 15)).unwrap();
                schedule_updates(updates_tx, nodes, node_id);
            }
        }
        PNodeType::Comparator(mode) => {
            let new_strength = calculate_comparator_output(mode, input_power, side_input_power);
            let old_strength = node.output_strength.load(Ordering::Relaxed);
            if new_strength != old_strength || mode == ComparatorMode::Compare {
                node.output_strength.store(new_strength, Ordering::Relaxed);
                let should_be_powered =
                    comparator_should_be_powered(mode, input_power, side_input_power);
                let powered = node.powered.load(Ordering::Relaxed);
                if powered && !should_be_powered {
                    node.powered.store(false, Ordering::Relaxed);
                    changes_tx.send(BlockChange::Power(node_id, 0)).unwrap();
                } else if !powered && should_be_powered {
                    node.powered.store(true, Ordering::Relaxed);
                    changes_tx.send(BlockChange::Power(node_id, 15)).unwrap();
                }
                schedule_updates(updates_tx, nodes, node_id);
            }
        }
        PNodeType::Lamp => {
            let lit = node.powered.load(Ordering::Relaxed);
            let should_be_lit = input_power > 0;
            if lit && !should_be_lit {
                node.powered.store(false, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 0)).unwrap();
            }
        }
        PNodeType::StoneButton => {
            let powered = node.powered.load(Ordering::Relaxed);
            if powered {
                node.powered.store(false, Ordering::Relaxed);
                changes_tx.send(BlockChange::Power(node_id, 0)).unwrap();
                schedule_updates(updates_tx, nodes, node_id);
            }
        }
        _ => warn!("Node {:?} should not be ticked!", node.ty),
    }
}
