use mchprs_world::TickPriority;

use super::node::{NodeId, NodeType};
use super::{comparator_output_power, schedule_tick, DirectBackend};
use crate::compile_graph::SignalStrength;

impl DirectBackend {
    // Benchmarks show that `tick_node` getting inlined into `tick` causes worse perf.
    #[inline(never)]
    pub fn tick_node(&mut self, node_id: NodeId) {
        let node = &mut self.nodes[node_id];
        node.pending_tick = false;

        match node.ty {
            NodeType::Repeater { delay, .. } => {
                if node.repeater_locked {
                    return;
                }

                let should_be_powered = node.default_inputs.is_powered();
                if node.is_powered() && !should_be_powered {
                    self.set_power_and_propagate(node_id, SignalStrength::ZERO);
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
                    self.set_power_and_propagate(node_id, SignalStrength::MAX);
                }
            }
            NodeType::Torch => {
                let should_be_powered = !node.default_inputs.is_powered();
                if node.is_powered() != should_be_powered {
                    self.set_power_and_propagate(node_id, should_be_powered.into());
                }
            }
            NodeType::Comparator {
                mode, far_input, ..
            } => {
                let power = comparator_output_power(node, mode, far_input);
                if power != node.power {
                    self.set_power_and_propagate(node_id, power);
                }
            }
            NodeType::Lamp => {
                let should_be_lit = node.default_inputs.is_powered();
                if node.is_powered() && !should_be_lit {
                    node.set_powered(false);
                }
            }
            NodeType::Button if node.is_powered() => {
                self.set_power_and_propagate(node_id, SignalStrength::ZERO);
            }
            _ => {}
        }
    }
}
