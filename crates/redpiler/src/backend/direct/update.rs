use mchprs_world::TickPriority;

use super::node::{NodeId, NodeType, Nodes};
use super::{comparator_output_power, schedule_tick, Event, TickScheduler};

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
            let should_be_locked = node.side_inputs.is_powered();
            if should_be_locked != node.repeater_locked {
                node.set_repeater_locked(should_be_locked);
            }
            if node.repeater_locked || node.pending_tick {
                return;
            }

            let should_be_powered = node.default_inputs.is_powered();
            if should_be_powered != node.is_powered() {
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
            let should_be_powered = !node.default_inputs.is_powered();
            if node.is_powered() != should_be_powered {
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
            let power = comparator_output_power(node, mode, far_input);
            if power != node.power {
                let priority = if facing_diode {
                    TickPriority::High
                } else {
                    TickPriority::Normal
                };
                schedule_tick(scheduler, node_id, node, 1, priority);
            }
        }
        NodeType::Lamp => {
            let should_be_lit = node.default_inputs.is_powered();
            let lit = node.is_powered();
            if lit && !should_be_lit {
                schedule_tick(scheduler, node_id, node, 2, TickPriority::Normal);
            } else if !lit && should_be_lit {
                node.set_powered(true);
            }
        }
        NodeType::Trapdoor => {
            let should_be_powered = node.default_inputs.is_powered();
            if node.is_powered() != should_be_powered {
                node.set_powered(should_be_powered);
            }
        }
        NodeType::Wire => {
            let input_power = node.default_inputs.power();
            if node.power != input_power {
                node.set_power(input_power);
            }
        }
        NodeType::NoteBlock { noteblock_id } => {
            let should_be_powered = node.default_inputs.is_powered();
            if node.is_powered() != should_be_powered {
                node.set_powered(should_be_powered);
                if should_be_powered {
                    events.push(Event::NoteBlockPlay { noteblock_id });
                }
            }
        }
        _ => {}
    }
}
