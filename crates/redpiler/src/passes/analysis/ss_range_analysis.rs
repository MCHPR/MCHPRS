//! # [`SSRangeAnalysis`]
//!
//! This analysis pass determines the possible range of signal strengths that can be produced by all
//! nodes
//!
//! 1. We give all nodes that have no inputs the default ss range, and propogate those ranges. The
//!    nodes left over after this are in cycles.
//! 2. Locking repeaters are given given default ss ranges. This should break most cycles in solid
//!    state pipelined logic.
//! 3. All left over nodes are given default ss ranges.
//!
//! TODO: handle cases where a cycle has a constrained input. Pulse extender example: button ->
//! comparator subtract by constant -> comparator loop

use crate::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeState, NodeType};
use crate::passes::{AnalysisInfo, AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use itertools::Itertools;
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_world::World;
use petgraph::graph::NodeIndex;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;
use std::iter;
use crate::backend::direct::calculate_comparator_output;

/// The possible output range of a node
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SSRange {
    /// The lower bound of the range (inclusive)
    pub low: u8,
    /// The upper bound of the range (inclusive)
    pub high: u8,
}

impl SSRange {
    pub const FULL: SSRange = SSRange { low: 0, high: 15 };

    pub fn constant(ss: u8) -> SSRange {
        SSRange { low: ss, high: ss }
    }

    pub fn dust_or(self, other: Self) -> Self {
        SSRange { low: self.low.max(other.low), high: self.high.max(other.high) }
    }

    pub fn contains(self, ss: u8) -> bool {
        self.low <= ss && ss <= self.high
    }

    pub fn contains_positive(self) -> bool {
        self.high > 0
    }

    pub fn with(self, ss: u8) -> Self {
        Self {
            low: self.low.min(ss),
            high: self.high.max(ss),
        }
    }

    pub fn insert(&mut self, ss: u8) {
        *self = self.with(ss);
    }

    /// Perform a saturating sub on each component of the range for ss decay
    fn decay(self, ss: u8) -> SSRange {
        SSRange {
            low: self.low.saturating_sub(ss),
            high: self.high.saturating_sub(ss),
        }
    }

    fn saturating_sub(self, other: SSRange) -> SSRange {
        SSRange {
            low: self.low.saturating_sub(other.high),
            high: self.high.saturating_sub(other.low),
        }
    }

    pub fn bool_signature(self, dist: u8) -> u16 {
        let Self {low, high} = self;
        let bitset = 0xffffu16 >> (15u8 + low - high) << low;
        bitset & (0xfffe << dist)
    }

    pub fn hex_signature(self, dist: u8) -> u16 {
        let Self {low, high} = self;
        let bitset = 0xffffu16 >> (15u8 + low - high) << low;
        (bitset & 1) | ((bitset & 0xfffe) >> dist)
    }
}


#[derive(Default)]
pub struct SSRangeInfo {
    ranges: Vec<Option<SSRange>>,
}

impl SSRangeInfo {
    /// Pre-allocate enough ranges for the entire graph
    fn reserve(&mut self, graph: &CompileGraph) {
        let len = graph.node_bound();
        self.ranges.extend(iter::repeat_n(None, len));
    }

    pub fn set_range(&mut self, node_idx: NodeIndex, range: SSRange) {
        let idx = node_idx.index();
        if idx >= self.ranges.len() {
            self.ranges
                .extend(iter::repeat_n(None, idx - self.ranges.len() + 1));
        }
        self.ranges[node_idx.index()] = Some(range);
    }

    pub fn get_range(&self, node_idx: NodeIndex) -> Option<SSRange> {
        self.ranges.get(node_idx.index()).copied().flatten()
    }

    fn extend_range_to_include(&mut self, node_idx: NodeIndex, ss: u8) {
        if let Some(range) = &mut self.ranges[node_idx.index()] {
            if range.low > ss {
                range.low = ss;
            }
            if range.high < ss {
                range.high = ss;
            }
        }
    }
}

impl AnalysisInfo for SSRangeInfo {}

pub struct SSRangeAnalysis;

impl<W: World> Pass<W> for SSRangeAnalysis {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let mut range_info = SSRangeInfo::default();
        range_info.reserve(graph);
        
        // First, we give all nodes with no inputs the default range
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            // TODO: check if this is correct if there are pending ticks
            let range = match node.ty {
                // Inputs
                NodeType::Button | NodeType::Lever | NodeType::PressurePlate => SSRange::constant(0).with(15),
                // Outputs
                NodeType::Trapdoor | NodeType::Lamp | NodeType::NoteBlock { .. } => SSRange::constant(0),
                // Hex components
                NodeType::Constant | NodeType::Comparator { .. } | NodeType::Wire | NodeType::Repeater { .. } | NodeType::Torch
                    => SSRange::constant(node.state.output_strength),
            };
            
            range_info.set_range(node_idx, range);
        }
        


        loop {
            let num_updated = narrow_iteration(graph, &mut range_info);
            if num_updated == 0 {
                break;
            }
        }

        analysis_infos.insert_analysis(range_info);
    }

    fn status_message(&self) -> &'static str {
        "Analyzing signal strength ranges"
    }
}

pub fn calc_possible_inputs(graph: &CompileGraph, range_info: &SSRangeInfo, idx: NodeIdx) -> (SSRange, SSRange) {
    let node = &graph[idx];
    let mut def = SSRange::constant(0);
    let mut side = SSRange::constant(0);
    for edge in graph.edges_directed(idx, Direction::Incoming) {
        let source = edge.source();
        let weight = edge.weight();
        let ss = weight.ss;
        let ty = weight.ty;
        let val = range_info.get_range(source).unwrap();
        let val = val.decay(ss);
        if ty == LinkType::Default {
            def = def.dust_or(val);
        } else {
            side = side.dust_or(val);
        }
    }

    if let NodeType::Comparator {
        far_input: Some(far_input),
        ..
    } = node.ty
    {
        def = if def == SSRange::constant(15) {
            SSRange::constant(15)
        } else if def.contains(15) {
            SSRange::constant(15).with(far_input)
        } else {
            SSRange::constant(far_input)
        };
    }

    (def, side)
}

fn calc_possible_outputs(graph: &CompileGraph, range_info: &SSRangeInfo, idx: NodeIdx) -> SSRange {
    let node = &graph[idx];
    let (def, side) = calc_possible_inputs(graph, range_info, idx);

    let mut outputs = SSRange::constant(node.state.output_strength);
    match node.ty {
        NodeType::Repeater { .. } => {
            if def.contains(0) {
                outputs.insert(0)
            }
            if def.contains_positive() {
                outputs.insert(15)
            }
        }
        NodeType::Torch => {
            if def.contains(0) {
                outputs.insert(15)
            }
            if def.contains_positive() {
                outputs.insert(0);
            }
        }
        NodeType::Comparator { mode, .. } => {
            for def_ss in 0..=15u8 {
                if !def.contains(def_ss) {
                    continue;
                }
                for side_ss in 0..=15u8 {
                    if !side.contains(side_ss) {
                        continue;
                    }
                    let output = calculate_comparator_output(mode, def_ss, side_ss);
                    outputs.insert(output);
                }
            }
        }
        NodeType::Wire => outputs = def,
        NodeType::Lamp
        | NodeType::Button
        | NodeType::Lever
        | NodeType::PressurePlate
        | NodeType::Trapdoor
        | NodeType::Constant
        | NodeType::NoteBlock { .. } => outputs = range_info.get_range(idx).unwrap(),
    }
    outputs
}

fn narrow_iteration(graph: &mut CompileGraph, range_info: &mut SSRangeInfo) -> usize {
    let mut updated = 0;
    for i in 0..graph.node_bound() {
        let idx = NodeIdx::new(i);
        if !graph.contains_node(idx) {
            continue;
        }
        let old = range_info.get_range(idx).unwrap();
        let new_possible_outputs = calc_possible_outputs(graph, range_info, idx);

        if new_possible_outputs != old {
            updated += 1;
            range_info.set_range(idx, new_possible_outputs);
        }
    }
    return updated;
}
