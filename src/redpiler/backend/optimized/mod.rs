//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

mod node;
mod optimization;

use self::node::WireNode;

use super::JITBackend;
use crate::blocks::{BlockEntity, BlockPos, ComparatorMode, Block};
use crate::plot::PlotWorld;
use crate::redpiler::{CompileNode, LinkType};
use crate::world::{TickEntry, TickPriority, World};
use log::warn;
use std::collections::HashMap;
use node::{NodeType, Node};

#[derive(Debug)]
struct RPTickEntry {
    ticks_left: u32,
    tick_priority: TickPriority,
    node: usize,
}

#[derive(Default)]
pub struct OptimizedBackend {
    nodes: Vec<Node>,
    wires: Vec<WireNode>,
    /// -1 when unchanged, block id when changed
    node_changes: Vec<i16>,
    to_be_ticked: Vec<RPTickEntry>,
    pos_map: HashMap<BlockPos, usize>,
}

impl OptimizedBackend {
    fn schedule_tick(&mut self, node_id: usize, delay: u32, priority: TickPriority) {
        self.nodes[node_id].pending_tick = true;
        self.to_be_ticked.push(RPTickEntry {
            node: node_id,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn update_children(&mut self, node_id: usize) {
        let node = &self.nodes[node_id];
        for i in 0..node.updates.len() {
            let update = self.nodes[node_id].updates[i];
            update_node(&mut self.to_be_ticked, &mut self.nodes, &mut self.node_changes, update);
        }
        update_node(&mut self.to_be_ticked, &mut self.nodes, &mut self.node_changes, node_id);
    }
}

impl JITBackend for OptimizedBackend {
    fn reset(&mut self, plot: &mut PlotWorld) {
        for entry in &self.to_be_ticked {
            plot.schedule_tick(
                self.nodes[entry.node].pos,
                entry.ticks_left,
                entry.tick_priority,
            );
        }

        for node in &self.nodes {
            if let NodeType::Comparator(_) = node.ty {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.output_power,
                };
                plot.set_block_entity(node.pos, block_entity);
            }
        }

        self.nodes.clear();
        self.pos_map.clear();
        self.to_be_ticked.clear();
    }

    fn on_use_block(&mut self, _plot: &mut PlotWorld, pos: BlockPos) {
        let node_id = self.pos_map[&pos];
        let node = &mut self.nodes[node_id];
        match node.ty {
            NodeType::StoneButton => {
                if node.output_power == 0 {
                    node.output_power = 15;
                    self.node_changes[node_id] = node.encode_generic(true) as i16;
                } else {
                    node.output_power = 0;
                    self.node_changes[node_id] = node.encode_generic(false) as i16;
                }
                self.schedule_tick(node_id, 10, TickPriority::Normal);
                self.update_children(node_id);
            }
            NodeType::Lever => {
                if node.output_power == 0 {
                    node.output_power = 15;
                    self.node_changes[node_id] = node.encode_generic(true) as i16;
                } else {
                    node.output_power = 0;
                    self.node_changes[node_id] = node.encode_generic(false) as i16;
                }
                self.update_children(node_id);
            }
            _ => warn!("Tried to use a {:?} redpiler node", node.ty),
        }
    }

    fn set_pressure_plate(&mut self, _plot: &mut PlotWorld, pos: BlockPos, powered: bool) {
        let node_id = self.pos_map[&pos];
        let node = &self.nodes[node_id];
        match node.ty {
            NodeType::StonePressurePlate => {
                self.node_changes[node_id] = node.encode_generic(powered) as i16;
                self.update_children(node_id);
            }
            _ => warn!("Tried to set pressure plate state for a {:?}", node.ty),
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

            let node = &mut self.nodes[node_id];
            match node.ty {
                NodeType::Repeater(_delay) => {
                    // Locked?
                    if node.diode_state {
                        continue;
                    }

                    let should_be_powered = input_power > 0;
                    let powered = node.output_power > 0;
                    if powered && !should_be_powered {
                        node.output_power = 0;
                        self.node_changes[node_id] = node.encode_repeater(false, false) as i16;
                        self.update_children(node_id);
                    } else if !powered {
                        node.output_power = 15;
                        self.node_changes[node_id] = node.encode_repeater(true, false) as i16;
                        self.update_children(node_id);
                    }
                }
                NodeType::Torch => {
                    let lit = node.output_power > 0;
                    let should_be_off = input_power > 0;
                    if lit && should_be_off {
                        node.output_power = 0;
                        self.node_changes[node_id] = node.encode_generic(false) as i16;
                        self.update_children(node_id);
                    } else if !lit && !should_be_off {
                        node.output_power = 15;
                        self.node_changes[node_id] = node.encode_generic(true) as i16;
                        self.update_children(node_id);
                    }
                }
                NodeType::Comparator(mode) => {
                    if let Some(far_override) = node.comparator_far_input {
                        if input_power < 15 {
                            input_power = far_override;
                        }
                    }
                    let comparator_output = node.output_power;
                    let new_strength =
                        calculate_comparator_output(mode, input_power, side_input_power);
                    let old_strength = comparator_output;
                    if new_strength != old_strength || mode == ComparatorMode::Compare {
                        let node = &mut self.nodes[node_id];
                        node.output_power = new_strength;
                        let should_be_powered = comparator_should_be_powered(
                            mode,
                            input_power,
                            side_input_power,
                        );
                        let powered = node.diode_state;
                        if powered && !should_be_powered {
                            node.diode_state = false;
                        } else if !powered && should_be_powered {
                            node.diode_state = true;
                        }
                        self.node_changes[node_id] = node.encode_generic(node.diode_state) as i16;
                        self.update_children(node_id);
                    }
                }
                NodeType::Lamp => {
                    let lit = node.output_power > 0;
                    let should_be_lit = input_power > 0;
                    if lit && !should_be_lit {
                        self.node_changes[node_id] = node.encode_generic(false) as i16;
                        self.nodes[node_id].output_power = 0;
                    }
                }
                NodeType::StoneButton => {
                    let powered = node.output_power > 0;
                    if powered {
                        self.node_changes[node_id] = node.encode_generic(false) as i16;
                        self.nodes[node_id].output_power = 0;
                    }
                    self.update_children(node_id);
                }
                _ => warn!("Node {:?} should not be ticked!", node.ty),
            }
        }
        self.to_be_ticked.drain(0..i);
    }

    fn compile(&mut self, compile_nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {

        println!("Started with {} nodes", compile_nodes.len());

        let mut nodes: HashMap<usize, Node> = HashMap::new();
        let mut wires = Vec::new();
        for (i, node) in compile_nodes.into_iter().enumerate() {
            if matches!(node.state, Block::RedstoneWire { .. }) {
                wires.push(node);
            } else {
                nodes.insert(i, node.into());
            }
        }
        println!("{} nodes after removing wires", nodes.len());

        optimization::constant_fold(&mut nodes);
        optimization::remove_redundant(&mut nodes);
        println!("{} nodes after optimization", nodes.len());

        let mut map = HashMap::new();
        for (id, node) in nodes {
            map.insert(id, self.nodes.len());
            self.nodes.push(node);
        }

        for node in &mut self.nodes {
            // Remove links to removed nodes
            node.inputs.retain(|l| map.contains_key(&l.end));
            node.updates.retain(|id| map.contains_key(id));
            // Fix links
            node.inputs.iter_mut().for_each(|l| l.end = map[&l.end]);
            node.updates.iter_mut().for_each(|id| *id = map[id]);
        }
        for mut wire in wires {
            if wire.inputs.iter().any(|l| !map.contains_key(&l.end)) {
                // Wire has an input that has been removed; ignore
                continue;
            }

            wire.inputs.iter_mut().for_each(|l| l.end = map[&l.end]);
            self.wires.push(WireNode {
                id_offset: match wire.state {
                    Block::RedstoneWire { mut wire } => {
                        wire.power = 0;
                        Block::RedstoneWire { wire }.get_id()
                    }
                    _ => unreachable!()
                },
                pos: wire.pos,
                power: match wire.state {
                    Block::RedstoneWire { wire } => wire.power,
                    _ => unreachable!()
                },
                inputs: wire.inputs.into_iter().map(Into::into).collect(),
            });
        }

        
        println!("Now {} nodes", self.nodes.len());
        // for foldable in foldable.iter().step_by(10000) {
        //     dbg!(&self.nodes[*foldable]);
        // }

        for (i, node) in self.nodes.iter().enumerate() {
            if matches!(node.ty, NodeType::StoneButton | NodeType::Lever | NodeType::StonePressurePlate) {
                self.pos_map.insert(node.pos, i);
            }
        }

        self.node_changes = vec![-1; self.nodes.len()];
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

    fn flush(&mut self, plot: &mut PlotWorld) {
        for (i, change) in self.node_changes.iter_mut().enumerate() {
            if *change >= 0 {
                plot.set_block_raw(self.nodes[i].pos, *change as u32);
            }
        }
        // let nodes = &self.nodes;
        // for wire in &mut self.wires {
        //     let input = wire.inputs.iter().map(|link| nodes[link.end].output_power.saturating_sub(link.weight)).max();
        //     if let Some(input) = input {
        //         if input != wire.power {
        //             wire.power = input;
        //             plot.set_block_raw(wire.pos, wire.encode());
        //         }
        //     }
        // }
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

fn update_node(to_be_ticked: &mut Vec<RPTickEntry>, nodes: &mut Vec<Node>, changes: &mut Vec<i16>, node_id: usize) {
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

    let node = &mut nodes[node_id];
    match node.ty {
        NodeType::Repeater(delay) => {
            let should_be_locked = side_input_power > 0;
            if !node.diode_state && should_be_locked {
                node.diode_state = true;
                changes[node_id] = node.encode_repeater(node.output_power > 0, true) as i16;
            } else if node.diode_state && !should_be_locked {
                node.diode_state = false;
                changes[node_id] = node.encode_repeater(node.output_power > 0, false) as i16;
            }

            if !node.diode_state && !node.pending_tick {
                let should_be_powered = input_power > 0;
                let powered = node.output_power > 0;
                if should_be_powered != powered {
                    let priority = if facing_diode {
                        TickPriority::Highest
                    } else if !should_be_powered {
                        TickPriority::Higher
                    } else {
                        TickPriority::High
                    };
                    schedule_tick(to_be_ticked, node_id, node, delay as u32, priority);
                }
            }
        }
        NodeType::Torch => {
            let lit = node.output_power > 0;
            if lit == (input_power > 0) && !node.pending_tick {
                schedule_tick(to_be_ticked, node_id, node, 1, TickPriority::Normal);
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
                || node.diode_state
                    != comparator_should_be_powered(mode, input_power, side_input_power)
            {
                let priority = if facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(to_be_ticked, node_id, node, 1, priority);
            }
        }
        NodeType::Lamp => {
            let lit = node.output_power > 0;
            let should_be_lit = input_power > 0;
            if lit && !should_be_lit {
                schedule_tick(to_be_ticked, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                node.output_power = 15;
                changes[node_id] = node.encode_generic(true) as i16;
            }
        }
        _ => {} // panic!("Node {:?} should not be updated!", node.state),
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
