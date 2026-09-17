use super::node::NodeId;
use super::*;

impl DirectBackend {
    // Benchmarks show that `tick_node` getting inlined into `tick` causes worse perf.
    #[inline(never)]
    pub fn tick_node(&mut self, node_id: NodeId) {
        let node = &mut self.nodes[node_id];
        node.pending_tick = false;

        match node.ty {
            NodeType::Repeater { delay, .. } => {
                if node.locked {
                    return;
                }

                let should_be_powered = get_bool_input(node);
                if node.is_powered() && !should_be_powered {
                    self.set_node_output(node_id, 0);
                } else if !node.is_powered() {
                    if !should_be_powered {
                        schedule_tick(
                            &mut self.scheduler,
                            node_id,
                            node,
                            delay as usize,
                            TickPriority::Higher,
                        );
                    }
                    self.set_node_output(node_id, 15);
                }
            }
            NodeType::Torch => {
                let should_be_powered = !get_bool_input(node);
                if node.is_powered() != should_be_powered {
                    self.set_node_output(node_id, bool_to_ss(should_be_powered));
                }
            }
            NodeType::Comparator {
                mode, far_input, ..
            } => {
                let (mut input_power, side_input_power) = get_all_input(node);
                if let Some(far_override) = far_input
                    && input_power < 15
                {
                    input_power = far_override.get();
                }
                let old_strength = node.output_strength;
                let new_strength = calculate_comparator_output(mode, input_power, side_input_power);
                if new_strength != old_strength {
                    self.set_node_output(node_id, new_strength);
                }
            }
            NodeType::Lamp => {
                let should_be_lit = get_bool_input(node);
                if node.is_powered() && !should_be_lit {
                    set_node_powered(node, false);
                }
            }
            NodeType::Button => {
                if node.is_powered() {
                    self.set_node_output(node_id, 0);
                }
            }
            _ => {} //unreachable!("Node {:?} should not be ticked!", node.ty),
        }
    }
}
