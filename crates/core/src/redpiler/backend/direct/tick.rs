use super::node::NodeId;
use super::*;

impl DirectBackend {
    pub fn tick_node(&mut self, node_id: NodeId) {
        self.nodes[node_id].pending_tick = false;
        let node = &self.nodes[node_id];

        match node.ty {
            NodeType::Repeater(delay) => {
                if node.locked {
                    return;
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
            _ => warn!("Node {:?} should not be ticked!", node.ty),
        }
    }
}
