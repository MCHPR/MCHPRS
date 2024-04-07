use mchprs_world::TickPriority;

use super::node::{NodeId, NodeType};
use super::*;

#[inline(always)]
pub(super) fn update_node(
    scheduler: &mut TickScheduler,
    events: &mut Vec<Event>,
    nodes: &mut Nodes,
    node_id: NodeId,
) {
    let node = &mut nodes[node_id];

    match node.ty {
        NodeType::Repeater {
            delay,
            facing_diode,
        } => {
            let should_be_locked = get_bool_side(node);
            if should_be_locked != node.locked {
                set_node_locked(node, should_be_locked);
            }
            if node.locked || node.pending_tick {
                return;
            }

            let should_be_powered = get_bool_input(node);
            if should_be_powered != node.powered {
                let priority = if facing_diode {
                    TickPriority::Highest
                } else if !should_be_powered {
                    TickPriority::Higher
                } else {
                    TickPriority::High
                };
                schedule_tick(scheduler, node_id, node, delay as usize, priority);
            }
        }
        NodeType::Torch => {
            if node.pending_tick {
                return;
            }
            let should_be_powered = !get_bool_input(node);
            if node.powered != should_be_powered {
                schedule_tick(scheduler, node_id, node, 1, TickPriority::Normal);
            }
        }
        NodeType::Comparator {
            mode,
            far_input,
            facing_diode,
        } => {
            if node.pending_tick {
                return;
            }
            let (mut input_power, side_input_power) = get_all_input(node);
            if let Some(far_override) = far_input {
                if input_power < 15 {
                    input_power = far_override.get();
                }
            }
            let old_strength = node.output_power;
            let output_power = calculate_comparator_output(mode, input_power, side_input_power);
            if output_power != old_strength {
                let priority = if facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(scheduler, node_id, node, 1, priority);
            }
        }
        NodeType::Lamp => {
            let should_be_lit = get_bool_input(node);
            let lit = node.powered;
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                set_node(node, true);
            }
        }
        NodeType::Trapdoor => {
            let should_be_powered = get_bool_input(node);
            if node.powered != should_be_powered {
                set_node(node, should_be_powered);
            }
        }
        NodeType::Wire => {
            let (input_power, _) = get_all_input(node);
            if node.output_power != input_power {
                node.output_power = input_power;
                node.changed = true;
            }
        }
        NodeType::NoteBlock { noteblock_id } => {
            let should_be_powered = get_bool_input(node);
            if node.powered != should_be_powered {
                set_node(node, should_be_powered);
                if should_be_powered {
                    events.push(Event::NoteBlockPlay { noteblock_id });
                }
            }
        }
        _ => {} // unreachable!("Node {:?} should not be updated!", node.ty),
    }
}
