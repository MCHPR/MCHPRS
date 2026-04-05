use crate::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeState, NodeType};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_world::World;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;
use tracing::trace;

pub struct ConstantFold;

impl<W: World> Pass<W> for ConstantFold {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        _: &mut AnalysisInfos,
    ) {
        let num_folded = fold(graph);
        trace!("Fold iteration: {} nodes", num_folded);
    }

    fn status_message(&self) -> &'static str {
        "Constant folding"
    }

    fn driver_key(&self) -> &'static str {
        "constant-fold"
    }
}

/// Returns true if the node was turned into a constant
fn fold_node(graph: &mut CompileGraph, idx: NodeIdx) -> bool {
    let mut default_power = 0;
    let mut side_power = 0;
    for edge in graph.edges_directed(idx, Direction::Incoming) {
        let constant = &graph[edge.source()];
        if constant.ty != NodeType::Constant {
            return false;
        }

        match edge.weight().ty {
            LinkType::Default => {
                default_power = default_power.max(
                    constant
                        .state
                        .output_strength
                        .saturating_sub(edge.weight().ss),
                )
            }
            LinkType::Side => {
                side_power = side_power.max(
                    constant
                        .state
                        .output_strength
                        .saturating_sub(edge.weight().ss),
                )
            }
        }
    }

    let new_power = match graph[idx].ty {
        NodeType::Comparator {
            mode, far_input, ..
        } => {
            if let Some(far_override) = far_input {
                if default_power < 15 {
                    default_power = far_override;
                }
            }
            match mode {
                ComparatorMode::Compare => {
                    if default_power >= side_power {
                        default_power
                    } else {
                        0
                    }
                }
                ComparatorMode::Subtract => default_power.saturating_sub(side_power),
            }
        }
        NodeType::Repeater { .. } => {
            if graph[idx].state.repeater_locked {
                graph[idx].state.output_strength
            } else if default_power > 0 {
                15
            } else {
                0
            }
        }
        NodeType::Torch => {
            if default_power > 0 {
                0
            } else {
                15
            }
        }
        _ => return false,
    };

    graph[idx].ty = NodeType::Constant;
    graph[idx].state = NodeState::ss(new_power);

    let mut incoming = graph.neighbors_directed(idx, Direction::Incoming).detach();
    while let Some(edge) = incoming.next_edge(graph) {
        graph.remove_edge(edge);
    }

    true
}

fn fold(graph: &mut CompileGraph) -> usize {
    let mut num_folded = 0;

    let mut worklist = Vec::new();

    for i in 0..graph.node_bound() {
        let idx = NodeIdx::new(i);
        if !graph.contains_node(idx) {
            continue;
        }

        worklist.push(idx);
        while let Some(idx) = worklist.pop() {
            if fold_node(graph, idx) {
                worklist.extend(graph.neighbors_directed(idx, Direction::Outgoing));
                num_folded += 1;
            }
        }
    }

    num_folded
}
