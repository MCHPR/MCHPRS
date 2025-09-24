//! # [`ConstantFold2`]
//!
//! This pass replaces nodes of constant output with a constant node
//! This pass requires narrow_outputs.rs to be ran first
//! This pass replaces constant_coalesce.rs and constant_fold.rs

use super::Pass;
use crate::compile_graph::{CompileGraph, CompileNode, NodeIdx, NodeState, NodeType};
use crate::passes::analysis::ss_range_analysis::{SSRange, SSRangeInfo};
use crate::passes::coalesce2::coalesce;
use crate::passes::AnalysisInfos;
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use petgraph::visit::NodeIndexable;

pub struct ConstantFold2;

impl<W: World> Pass<W> for ConstantFold2 {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let range_info: &mut SSRangeInfo = analysis_infos.get_analysis_mut().unwrap();

        let constant = graph.add_node(CompileNode {
            ty: NodeType::Constant,
            block: None,
            state: NodeState::ss(15),
            is_input: false,
            is_output: false,
            annotations: Default::default(),
        });

        range_info.set_range(constant, SSRange::constant(15));



        for i in 0..graph.node_bound() {
            let idx = NodeIdx::new(i);
            if idx == constant || !graph.contains_node(idx) {
                continue;
            }
            let node = &graph[idx];

            if !node.is_removable() {
                continue;
            }

            let possible_outputs = range_info.get_range(idx).unwrap();
            if possible_outputs.low() != possible_outputs.high() {
                continue;
            };
            if possible_outputs.low() == 0 {
                graph.remove_node(idx);
            } else {
                coalesce(graph, idx, constant, 15 - possible_outputs.low());
            }
        }
    }

    fn status_message(&self) -> &'static str {
        "Replacing nodes of constant output with constants"
    }
}
