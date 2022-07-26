//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::JITBackend;
use crate::blocks::{Block, BlockEntity, BlockPos, ComparatorMode};
use crate::plot::PlotWorld;
use crate::redpiler::{CompileNode, Link, LinkType};
use crate::world::{TickEntry, TickPriority, World};
use log::warn;
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

#[derive(Debug, Clone)]
pub struct Node {
    pos: BlockPos,
    inputs: Vec<Link>,
    facing_diode: bool,
    comparator_far_input: Option<u8>,

    state: Block,
    updates: Vec<usize>,
    output_power: u8,
    comparator_output: u8,
    changed: bool,
    pending_tick: bool,
}

impl Node {
    fn update_output_power(&mut self) {
        self.output_power = match self.state {
            Block::RedstoneComparator { .. } => self.comparator_output,
            Block::RedstoneTorch { lit } => lit.then(|| 15).unwrap_or(0),
            Block::RedstoneWallTorch { lit, .. } => lit.then(|| 15).unwrap_or(0),
            Block::RedstoneRepeater { repeater } => repeater.powered.then(|| 15).unwrap_or(0),
            Block::Lever { lever } => lever.powered.then(|| 15).unwrap_or(0),
            Block::StoneButton { button } => button.powered.then(|| 15).unwrap_or(0),
            Block::RedstoneBlock {} => 15,
            Block::Observer { observer } => if observer.powered {15} else {0},
            Block::StonePressurePlate { powered } => powered.then(|| 15).unwrap_or(0),
            s if s.has_comparator_override() => self.comparator_output,
            _ => 0,
        }
    }
}

impl From<CompileNode> for Node {
    fn from(node: CompileNode) -> Self {
        let mut n = Node {
            pos: node.pos,
            state: node.state,
            inputs: node.inputs,
            updates: node.updates,
            output_power: 0,
            comparator_output: node.comparator_output,
            facing_diode: node.facing_diode,
            comparator_far_input: node.comparator_far_input,
            pending_tick: false,
            changed: false,
        };
        n.update_output_power();
        n
    }
}

struct RPTickEntry {
    ticks_left: u32,
    tick_priority: TickPriority,
    node: usize,
}

#[derive(Default)]
pub struct DirectBackend {
    nodes: Vec<Node>,
    to_be_ticked: Vec<RPTickEntry>,
    pos_map: HashMap<BlockPos, usize>,
}

impl DirectBackend {
    fn schedule_tick(&mut self, node_id: usize, delay: u32, priority: TickPriority) {
        self.nodes[node_id].pending_tick = true;
        self.to_be_ticked.push(RPTickEntry {
            node: node_id,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn set_node(&mut self, node_id: usize, new_block: Block, update: bool) {
        let node = &mut self.nodes[node_id];
        node.state = new_block;
        node.changed = true;
        if update {
            node.update_output_power();
            for i in 0..node.updates.len() {
                let update = self.nodes[node_id].updates[i];
                update_node(&mut self.to_be_ticked, &mut self.nodes, update);
            }
            update_node(&mut self.to_be_ticked, &mut self.nodes, node_id);
        }
    }

    fn set_node_and_update_neighbors(&mut self, node_id: usize, new_block: Block) {
        let node = &mut self.nodes[node_id];
        node.state = new_block;
        node.changed = true;
        node.update_output_power();
        for i in 0..node.updates.len() {
            let update = self.nodes[node_id].updates[i];
            update_node(&mut self.to_be_ticked, &mut self.nodes, update);
        }
    }
}

impl JITBackend for DirectBackend {
    fn reset(&mut self, plot: &mut PlotWorld, io_only: bool) {
        for entry in &self.to_be_ticked {
            plot.schedule_tick(
                self.nodes[entry.node].pos,
                entry.ticks_left,
                entry.tick_priority,
            );
        }

        for node in &self.nodes {
            if let Block::RedstoneComparator { .. } = node.state {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.comparator_output,
                };
                plot.set_block_entity(node.pos, block_entity);
            }

            if io_only && !is_io_block(node.state) {
                plot.set_block(node.pos, node.state);
            }
        }

        self.nodes.clear();
        self.pos_map.clear();
        self.to_be_ticked.clear();
    }

    fn on_use_block(&mut self, _plot: &mut PlotWorld, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.state {
            Block::StoneButton { mut button } => {
                button.powered = !button.powered;
                self.schedule_tick(node_id, 10, TickPriority::Normal);
                self.set_node(node_id, Block::StoneButton { button }, true);
            }
            Block::Lever { mut lever } => {
                lever.powered = !lever.powered;
                self.set_node(node_id, Block::Lever { lever }, true);
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.state),
        }
    }

    fn set_pressure_plate(&mut self, _plot: &mut PlotWorld, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.state {
            Block::StonePressurePlate { .. } => {
                self.set_node(node_id, Block::StonePressurePlate { powered }, true);
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.state),
        }
    }

    fn tick(&mut self, _plot: &mut PlotWorld) {
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority));
        for pending in &mut self.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }

        let mut i = 0;
        for _ in 0..self.to_be_ticked.len() {
            let entry = &self.to_be_ticked[i];
            if entry.ticks_left != 0 {
                break;
            }
            i += 1;

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
                *power = (*power).max(
                    self.nodes[link.end]
                        .output_power
                        .saturating_sub(link.weight),
                );
            }

            match node.state {
                Block::RedstoneRepeater { mut repeater } => {
                    if repeater.locked {
                        continue;
                    }

                    let should_be_powered = input_power > 0;
                    if repeater.powered && !should_be_powered {
                        repeater.powered = false;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, true);
                    } else if !repeater.powered {
                        repeater.powered = true;
                        self.set_node(node_id, Block::RedstoneRepeater { repeater }, true);
                    }
                }
                Block::Observer { mut observer } => {
                    if !observer.powered {
                        self.schedule_tick(node_id, 1, TickPriority::Normal);
                    }

                    observer.powered = !observer.powered;
                    self.set_node_and_update_neighbors(node_id, Block::Observer { observer });
                }
                Block::RedstoneTorch { lit } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: false }, true);
                    } else if !lit && !should_be_off {
                        self.set_node(node_id, Block::RedstoneTorch { lit: true }, true);
                    }
                }
                Block::RedstoneWallTorch { lit, facing } => {
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        self.set_node(
                            node_id,
                            Block::RedstoneWallTorch { lit: false, facing },
                            true,
                        );
                    } else if !lit && !should_be_off {
                        self.set_node(
                            node_id,
                            Block::RedstoneWallTorch { lit: true, facing },
                            true,
                        );
                    }
                }
                Block::RedstoneComparator { mut comparator } => {
                    if let Some(far_override) = self.nodes[node_id].comparator_far_input {
                        if input_power < 15 {
                            input_power = far_override;
                        }
                    }
                    let comparator_output = node.comparator_output;
                    let new_strength =
                        calculate_comparator_output(comparator.mode, input_power, side_input_power);
                    let old_strength = comparator_output;
                    if new_strength != old_strength || comparator.mode == ComparatorMode::Compare {
                        self.nodes[node_id].comparator_output = new_strength;
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
                        self.set_node(node_id, Block::RedstoneComparator { comparator }, true);
                    }
                }
                Block::RedstoneLamp { lit } => {
                    let should_be_lit = input_power > 0;
                    if lit && !should_be_lit {
                        self.set_node_and_update_neighbors(node_id, Block::RedstoneLamp { lit: false });
                    }
                }
                Block::StoneButton { mut button } => {
                    if button.powered {
                        button.powered = false;
                        self.set_node(node_id, Block::StoneButton { button }, true);
                    }
                }
                _ => warn!("Node {:?} should not be ticked!", node.state),
            }
        }
        self.to_be_ticked.drain(0..i);
    }

    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {
        for (i, node) in nodes.iter().enumerate() {
            self.pos_map.insert(node.pos, i);
        }
        self.nodes = nodes.into_iter().map(Into::into).collect();
        for entry in ticks {
            if let Some(node) = self.pos_map.get(&entry.pos) {
                self.to_be_ticked.push(RPTickEntry {
                    ticks_left: entry.ticks_left,
                    tick_priority: entry.tick_priority,
                    node: *node,
                });
            }
        }
        // Dot file output
        // println!("{}", self);
    }

    fn flush(&mut self, plot: &mut PlotWorld, io_only: bool) {
        for node in &mut self.nodes {
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
fn set_node_and_update_neighbors(to_be_ticked: &mut Vec<RPTickEntry>, nodes: &mut [Node], node_id: usize, new_state: Block) {
    let node = &mut nodes[node_id];
    node.state = new_state;
    node.changed = true;
    // Not strictly needed as this function is only called by blocks that have no redstone output
    node.update_output_power(); 
    for i in 0..node.updates.len() {
        let update = nodes[node_id].updates[i];
        update_node(to_be_ticked, nodes, update);
    }
}

fn schedule_tick(
    to_be_ticked: &mut Vec<RPTickEntry>,
    node_id: usize,
    node: &mut Node,
    delay: u32,
    priority: TickPriority,
) {
    node.pending_tick = true;
    to_be_ticked.push(RPTickEntry {
        node: node_id,
        ticks_left: delay,
        tick_priority: priority,
    });
}

fn update_node(to_be_ticked: &mut Vec<RPTickEntry>, nodes: &mut [Node], node_id: usize) {
    let node = &nodes[node_id];

    let mut input_power = 0;
    let mut side_input_power = 0;
    for link in &node.inputs {
        let power = match link.ty {
            LinkType::Default => &mut input_power,
            LinkType::Side => &mut side_input_power,
        };
        *power = (*power).max(nodes[link.end].output_power.saturating_sub(link.weight));
    }

    let facing_diode = node.facing_diode;
    let comparator_output = node.comparator_output;

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
                    let priority = if facing_diode {
                        TickPriority::Highest
                    } else if !should_be_powered {
                        TickPriority::Higher
                    } else {
                        TickPriority::High
                    };
                    schedule_tick(to_be_ticked, node_id, node, repeater.delay as u32, priority);
                }
            }
        }
        Block::Observer { .. } => {
            if  !node.pending_tick {
                schedule_tick(to_be_ticked, node_id, node, 1, TickPriority::Normal)
            }
        }
        Block::RedstoneTorch { lit } | Block::RedstoneWallTorch { lit, .. } => {
            if lit == (input_power > 0) && !node.pending_tick {
                schedule_tick(to_be_ticked, node_id, node, 1, TickPriority::Normal);
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
            let old_strength = comparator_output;
            if output_power != old_strength
                || comparator.powered
                    != comparator_should_be_powered(comparator.mode, input_power, side_input_power)
            {
                let priority = if facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(to_be_ticked, node_id, node, 1, priority);
            }
        }
        Block::RedstoneLamp { lit } => {
            let should_be_lit = input_power > 0;
            if lit && !should_be_lit {
                schedule_tick(to_be_ticked, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node_and_update_neighbors(to_be_ticked, nodes, node_id, Block::RedstoneLamp { lit: true });
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
                set_node_and_update_neighbors(to_be_ticked, nodes, node_id, new_block);
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
        for (id, node) in self.nodes.iter().enumerate() {
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
                    link.end, link.start, link.weight, color
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
