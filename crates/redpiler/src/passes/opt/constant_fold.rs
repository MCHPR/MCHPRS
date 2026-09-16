use crate::compile_graph::{CompileGraph, Direction, NodeIdx, NodeState, NodeType};
use crate::passes::{AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
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

/// A node whose inputs are all constant is never updated again, so it keeps its current output.
/// Returns true if the node was turned into a constant
fn fold_node(graph: &mut CompileGraph, idx: NodeIdx) -> bool {
    let node = &graph[idx];
    if node.state.pending_tick
        || !matches!(
            node.ty,
            NodeType::Comparator { .. } | NodeType::Repeater { .. } | NodeType::Torch
        )
    {
        return false;
    }
    if graph
        .neighbors(idx, Direction::Incoming)
        .any(|input| graph[input].ty != NodeType::Constant)
    {
        return false;
    }

    graph[idx].ty = NodeType::Constant;
    graph[idx].state = NodeState::ss(graph[idx].state.output_strength);

    let mut incoming = graph.neighbors(idx, Direction::Incoming).detach();
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
                worklist.extend(graph.neighbors(idx, Direction::Outgoing));
                num_folded += 1;
            }
        }
    }

    num_folded
}
