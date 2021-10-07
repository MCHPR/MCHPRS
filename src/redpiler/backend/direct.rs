//! The direct backend does not do code generation and operates on the `CompileNode` graph directly

use super::{JITBackend, JITResetData};
use crate::blocks::{Block, BlockEntity, BlockPos, ComparatorMode};
use crate::redpiler::{CompileNode, LinkType};
use crate::world::{TickEntry, TickPriority};
use log::warn;
use std::collections::HashMap;
use std::fmt;

struct RPTickEntry {
    ticks_left: u32,
    tick_priority: TickPriority,
    node: usize,
}

#[derive(Default)]
pub struct DirectBackend {
    change_queue: Vec<(BlockPos, Block)>,
    nodes: Vec<CompileNode>,
    to_be_ticked: Vec<RPTickEntry>,
    pos_map: HashMap<BlockPos, usize>,
}

impl DirectBackend {
    fn schedule_tick(&mut self, node_id: usize, delay: u32, priority: TickPriority) {
        self.to_be_ticked.push(RPTickEntry {
            node: node_id,
            ticks_left: delay,
            tick_priority: priority,
        });
    }

    fn pending_tick_at(&mut self, node: usize) -> bool {
        self.to_be_ticked.iter().any(|e| e.node == node)
    }

    fn set_node(&mut self, node_id: usize, new_block: Block, update: bool) {
        let node = &mut self.nodes[node_id];
        node.state = new_block;
        self.change_queue.push((node.pos, new_block));
        if update {
            for i in 0..node.updates.len() {
                let update = self.nodes[node_id].updates[i];
                self.update_node(update);
            }
            self.update_node(node_id);
        }
    }

    fn comparator_should_be_powered(
        &mut self,
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

    fn calculate_comparator_output(
        &mut self,
        mode: ComparatorMode,
        input_strength: u8,
        power_on_sides: u8,
    ) -> u8 {
        if mode == ComparatorMode::Subtract {
            input_strength.saturating_sub(power_on_sides)
        } else if input_strength >= power_on_sides {
            input_strength
        } else {
            0
        }
    }

    fn update_node(&mut self, node_id: usize) {
        let node = &self.nodes[node_id];

        let mut input_power = 0;
        let mut side_input_power = 0;
        for link in &node.inputs {
            let power = match link.ty {
                LinkType::Default => &mut input_power,
                LinkType::Side => &mut side_input_power,
            };
            *power = (*power).max(
                self.nodes[link.end]
                    .get_output_power()
                    .saturating_sub(link.weight),
            );
        }

        let facing_diode = node.facing_diode;
        let comparator_output = node.comparator_output;

        match node.state {
            Block::RedstoneRepeater { mut repeater } => {
                let should_be_locked = side_input_power > 0;
                if !repeater.locked && should_be_locked {
                    repeater.locked = true;
                    self.set_node(node_id, Block::RedstoneRepeater { repeater }, false);
                } else if repeater.locked && !should_be_locked {
                    repeater.locked = false;
                    self.set_node(node_id, Block::RedstoneRepeater { repeater }, false);
                }

                if !repeater.locked && !self.pending_tick_at(node_id) {
                    let should_be_powered = input_power > 0;
                    if should_be_powered != repeater.powered {
                        let priority = if facing_diode {
                            TickPriority::Highest
                        } else if !should_be_powered {
                            TickPriority::Higher
                        } else {
                            TickPriority::High
                        };
                        self.schedule_tick(node_id, repeater.delay as u32, priority);
                    }
                }
            }
            Block::RedstoneTorch { lit } | Block::RedstoneWallTorch { lit, .. } => {
                if lit == (input_power > 0) && !self.pending_tick_at(node_id) {
                    self.schedule_tick(node_id, 1, TickPriority::Normal);
                }
            }
            Block::RedstoneComparator { comparator } => {
                if self.pending_tick_at(node_id) {
                    return;
                }
                let output_power = self.calculate_comparator_output(
                    comparator.mode,
                    input_power,
                    side_input_power,
                );
                let old_strength = comparator_output;
                if output_power != old_strength
                    || comparator.powered
                        != self.comparator_should_be_powered(
                            comparator.mode,
                            input_power,
                            side_input_power,
                        )
                {
                    let priority = if facing_diode {
                        TickPriority::High
                    } else {
                        TickPriority::Normal
                    };
                    self.schedule_tick(node_id, 1, priority);
                }
            }
            Block::RedstoneLamp { lit } => {
                let should_be_lit = input_power > 0;
                if lit && !should_be_lit {
                    self.schedule_tick(node_id, 2, TickPriority::Normal);
                } else if !lit && should_be_lit {
                    self.set_node(node_id, Block::RedstoneLamp { lit: true }, false);
                }
            }
            Block::RedstoneWire { mut wire } => {
                if wire.power != input_power {
                    wire.power = input_power;
                    self.set_node(node_id, Block::RedstoneWire { wire }, false);
                }
            }
            _ => {} // panic!("Node {:?} should not be updated!", node.state),
        }
    }
}

impl JITBackend for DirectBackend {
    fn reset(&mut self) -> JITResetData {
        let mut ticks = Vec::new();
        for entry in self.to_be_ticked.drain(..) {
            ticks.push(TickEntry {
                ticks_left: entry.ticks_left,
                tick_priority: entry.tick_priority,
                pos: self.nodes[entry.node].pos,
            });
        }

        let mut block_entities = Vec::new();
        for node in &self.nodes {
            if let Block::RedstoneComparator { .. } = node.state {
                let block_entity = BlockEntity::Comparator {
                    output_strength: node.comparator_output,
                };
                block_entities.push((node.pos, block_entity));
            }
        }

        self.nodes.clear();
        self.pos_map.clear();
        self.to_be_ticked.clear();
        self.change_queue.clear();

        JITResetData {
            tick_entries: ticks,
            block_entities,
        }
    }

    fn on_use_block(&mut self, pos: BlockPos) {
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

    fn tick(&mut self) {
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.tick_priority));
        for pending in &mut self.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.to_be_ticked.first().map_or(1, |e| e.ticks_left) == 0 {
            let entry = self.to_be_ticked.remove(0);
            let node_id = entry.node;
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
                        .get_output_power()
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
                    let comparator_output = node.comparator_output;
                    let new_strength = self.calculate_comparator_output(
                        comparator.mode,
                        input_power,
                        side_input_power,
                    );
                    let old_strength = comparator_output;
                    if new_strength != old_strength || comparator.mode == ComparatorMode::Compare {
                        self.nodes[node_id].comparator_output = new_strength;
                        let should_be_powered = self.comparator_should_be_powered(
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
                        self.set_node(node_id, Block::RedstoneLamp { lit: false }, false);
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
    }

    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {
        for (i, node) in nodes.iter().enumerate() {
            self.pos_map.insert(node.pos, i);
        }
        self.nodes = nodes;
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

    fn block_changes(&mut self) -> &mut Vec<(BlockPos, Block)> {
        &mut self.change_queue
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
