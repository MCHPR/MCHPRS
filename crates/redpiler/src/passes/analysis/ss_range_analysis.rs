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

use crate::backend::direct::calculate_comparator_output;
use crate::compile_graph::{CompileGraph, LinkType, NodeIdx, NodeType};
use crate::passes::{AnalysisInfo, AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_world::World;
use petgraph::graph::NodeIndex;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;
use std::iter;

/// The possible output range of a node
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SSRange(u16);

impl SSRange {
    pub const FULL: Self = Self(0xffff);
    pub const BOOL: Self = Self(0x8001);

    #[inline(always)]
    pub const fn constant(ss: u8) -> SSRange {
        SSRange(1 << ss)
    }

    #[inline(always)]
    pub const fn dust_or(self, other: Self) -> Self {
        let a = self.0;
        let b = other.0;

        let a_lsb = a & (0u16.wrapping_sub(a));
        let a_mask = !a_lsb.saturating_sub(1);

        let b_lsb = b & (0u16.wrapping_sub(b));
        let b_mask = !b_lsb.saturating_sub(1);

        Self((a | b) & a_mask & b_mask)
    }

    #[inline(always)]
    pub const fn contains(self, ss: u8) -> bool {
        (self.0 & (1 << ss)) != 0
    }

    #[inline(always)]
    pub const fn contains_positive(self) -> bool {
        self.0 & 0xfffe != 0
    }

    #[inline(always)]
    pub const fn with(self, ss: u8) -> Self {
        debug_assert!(ss <= 15);
        Self(self.0 | (1 << ss))
    }

    #[inline(always)]
    pub const fn insert(&mut self, ss: u8) {
        *self = self.with(ss);
    }

    /// Perform a saturating sub on each component of the range for ss decay
    #[inline(always)]
    const fn decay(self, ss: u8) -> SSRange {
        Self((self.0 & 1) | (self.0 >> ss))
    }

    #[inline(always)]
    pub const fn low(self) -> u8 {
        (self.0.trailing_zeros() as u8) & 15
    }

    #[inline(always)]
    pub const fn high(self) -> u8 {
        debug_assert!(self.0 != 0);
        15 - self.0.leading_zeros() as u8
    }

    #[inline(always)]
    pub const fn bool_signature(self, dist: u8) -> u16 {
        // dist as u16
        self.0 & (0xfffe << dist)
    }

    #[inline(always)]
    pub const fn dist_from_bool_signature(self, sig: u16) -> u8 {
        // sig as u8
        Self(1 | (self.0 & !sig)).high()
    }

    #[inline(always)]
    pub const fn hex_signature(self, dist: u8) -> u16 {
        dist as u16
        // (self.0 & 1) | (self.0 >> dist)
    }

    #[inline(always)]
    pub const fn dist_from_hex_signature(self, sig: u16) -> u8 {
        // let a: i8 = Self(1 | self.0).high() as i8;
        // let b: i8 =  if sig == 0 {-1} else {Self(sig).high() as i8};
        // (a - b) as u8
        sig as u8
    }
}

#[test]
fn test_signature() {
    for example in (0..=u16::MAX).map(SSRange) {
        for dist in 0..=15u8 {
            let bin_sig = example.bool_signature(dist);
            let hex_sig = example.hex_signature(dist);

            let bin_dist = example.dist_from_bool_signature(bin_sig);
            let bin_sig2 = example.bool_signature(bin_dist);

            let hex_dist = example.dist_from_hex_signature(hex_sig);
            let hex_sig2 = example.hex_signature(hex_dist);

            // Assert recovered distance results in the same signature
            assert_eq!(bin_sig, bin_sig2);
            assert_eq!(hex_sig, hex_sig2);

            for i in 0..=15u8 {
                if example.0 & (1 << i) == 0 {
                    continue;
                }

                let output = i.saturating_sub(dist);
                let bin_output = i > bin_dist;
                let hex_output = i.saturating_sub(hex_dist);

                // Assert for every input power using the recovered distance has the same result
                assert_eq!(bin_output, output > 0);
                assert_eq!(hex_output, output);
            }
        }
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
        // Giving nodes a full range speeds up this pass significantly;
        // but might result in the SSRanges being a larger superset of their actual value, limiting optimization.
        // In practice this does however not seem to matter.
        let setup_with_full_range = true;

        let mut range_info = SSRangeInfo::default();
        range_info.reserve(graph);

        // First, we give all nodes with no inputs the default range
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            // TODO: check if this is correct if there are pending ticks
            let range = match node.ty {
                // Inputs
                NodeType::Button | NodeType::Lever | NodeType::PressurePlate => SSRange::BOOL,
                // Outputs
                NodeType::Trapdoor | NodeType::Lamp | NodeType::NoteBlock { .. } => {
                    SSRange::constant(0)
                }
                // Hex components
                NodeType::Comparator { .. } | NodeType::Wire => {
                    if setup_with_full_range {
                        SSRange::FULL
                    } else {
                        SSRange::constant(node.state.output_strength)
                    }
                }
                NodeType::Repeater { .. } | NodeType::Torch => {
                    if setup_with_full_range {
                        SSRange::BOOL
                    } else {
                        SSRange::constant(node.state.output_strength)
                    }
                }
                NodeType::Constant => SSRange::constant(node.state.output_strength),
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

pub fn calc_possible_inputs(
    graph: &CompileGraph,
    range_info: &SSRangeInfo,
    idx: NodeIdx,
) -> (SSRange, SSRange) {
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
