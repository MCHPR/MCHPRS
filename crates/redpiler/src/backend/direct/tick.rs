use super::node::NodeId;
use super::*;

impl DirectBackend {
    pub fn tick_node(&mut self, node_id: NodeId) {
        let node = &mut self.nodes[node_id];
        node.pending_tick = false;

        match node.ty {
            NodeType::Repeater { delay, .. } => {
                if node.locked {
                    return;
                }

                let should_be_powered = get_bool_input(node);
                if node.powered && !should_be_powered {
                    self.set_node(node_id, false, 0);
                } else if !node.powered {
                    if !should_be_powered {
                        schedule_tick(
                            &mut self.scheduler,
                            node_id,
                            node,
                            delay as usize,
                            TickPriority::Higher,
                        );
                    }
                    self.set_node(node_id, true, 15);
                }
            }
            NodeType::Torch => {
                let should_be_powered = !get_bool_input(node);
                if node.powered != should_be_powered {
                    self.set_node(node_id, should_be_powered, bool_to_ss(should_be_powered));
                }
            }
            NodeType::Comparator {
                mode, far_input, ..
            } => {
                let (mut input_power, side_input_power) = get_all_input(node);
                if let Some(far_override) = far_input {
                    if input_power < 15 {
                        input_power = far_override.get();
                    }
                }
                let old_strength = node.output_power;
                let new_strength = calculate_comparator_output(mode, input_power, side_input_power);
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
            _ => {} //unreachable!("Node {:?} should not be ticked!", node.ty),
        }
    }
}
