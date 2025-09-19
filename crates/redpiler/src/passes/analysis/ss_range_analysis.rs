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

use crate::compile_graph::{CompileGraph, LinkType, NodeState, NodeType};
use crate::passes::{AnalysisInfo, AnalysisInfos, Pass};
use crate::{CompilerInput, CompilerOptions};
use itertools::Itertools;
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_world::World;
use petgraph::graph::NodeIndex;
use petgraph::visit::{EdgeRef, NodeIndexable};
use petgraph::Direction;
use std::iter;

/// The possible output range of a node
#[derive(Clone, Copy, Debug)]
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
        (0xffffu16 >> (15u8 + low - high) << low) & (0xfffe << dist)
    }

    pub fn hex_signature(self, dist: u8) -> u16 {
        dist as u16
    }
}

fn range_to_bitset(low: u8, high: u8) -> u16 {
    (0xffff << (low + high)) >> low
}

fn bitset_to_range(bitset: u16) -> (u8, u8) {
    let low = (bitset.trailing_zeros() as u8) & 15;
    let high = bitset.checked_ilog2().unwrap_or(0) as u8;
    (low, high)
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
            let first_edge = graph.edges_directed(node_idx, Direction::Incoming).next();
            if first_edge.is_none() {
                let range = Self::range_for_no_inputs(&node.ty, &node.state);
                range_info.set_range(node_idx, range);
                Self::propogate_ss_ranges(graph, &mut range_info, node_idx);
            }
        }

        // Give left over locking repeaters a full range
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            if !matches!(node.ty, NodeType::Repeater { .. })
                || range_info.get_range(node_idx).is_some()
            {
                continue;
            }

            let first_side_edge = graph
                .edges_directed(node_idx, Direction::Incoming)
                .find(|edge| edge.weight().ty == LinkType::Side);
            if first_side_edge.is_some() {
                range_info.set_range(node_idx, SSRange::FULL);
                Self::propogate_ss_ranges(graph, &mut range_info, node_idx);
            }
        }

        // Give all left over nodes a full range
        for node_idx in graph.node_indices() {
            if range_info.get_range(node_idx).is_none() {
                let node = &graph[node_idx];

                let (default_range, side_range) =
                    Self::collect_input_range(graph, &mut range_info, node_idx, true).unwrap();
                let output_range =
                    Self::evaluate_with_range(&node.ty, &node.state, default_range, side_range);

                range_info.set_range(node_idx, output_range);
                Self::propogate_ss_ranges(graph, &mut range_info, node_idx);
            }
        }

        // Handle transient states
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            range_info.extend_range_to_include(node_idx, node.state.output_strength);
        }

        analysis_infos.insert_analysis(range_info);
    }

    fn status_message(&self) -> &'static str {
        "Analyzing signal strength ranges"
    }
}

impl SSRangeAnalysis {
    fn propogate_ss_ranges(graph: &CompileGraph, range_info: &mut SSRangeInfo, from: NodeIndex) {
        let mut queue = graph
            .neighbors_directed(from, Direction::Outgoing)
            .collect_vec();
        while let Some(node_idx) = queue.pop() {
            if range_info.get_range(node_idx).is_some() {
                continue;
            }

            let Some((default_range, side_range)) =
                Self::collect_input_range(graph, range_info, node_idx, false)
            else {
                continue;
            };

            let node = &graph[node_idx];
            let output_range =
                Self::evaluate_with_range(&node.ty, &node.state, default_range, side_range);
            range_info.set_range(node_idx, output_range);
            queue.extend(graph.neighbors_directed(node_idx, Direction::Outgoing));
        }
    }

    fn collect_input_range(
        graph: &CompileGraph,
        range_info: &mut SSRangeInfo,
        node_idx: NodeIndex,
        allow_missing: bool,
    ) -> Option<(SSRange, SSRange)> {
        fn reduce_range(acc: &mut Option<SSRange>, range: SSRange) {
            if let Some(acc) = acc.as_mut() {
                acc.low = acc.low.min(range.low);
                acc.high = acc.high.max(range.high);
            } else {
                *acc = Some(range);
            }
        }

        let mut default_range = None;
        let mut side_range = None;
        for edge in graph.edges_directed(node_idx, Direction::Incoming) {
            let source_idx = edge.source();
            let link = edge.weight();
            let src_range = range_info.get_range(source_idx).or({
                if allow_missing {
                    Some(SSRange::FULL)
                } else {
                    None
                }
            })?;
            let src_range = src_range.decay(link.ss);

            let acc = match link.ty {
                LinkType::Default => &mut default_range,
                LinkType::Side => &mut side_range,
            };
            reduce_range(acc, src_range);
        }
        let default_range = default_range.unwrap_or(SSRange::constant(0));
        let side_range = side_range.unwrap_or(SSRange::constant(0));
        Some((default_range, side_range))
    }

    fn evaluate_with_range(
        ty: &NodeType,
        state: &NodeState,
        default_range: SSRange,
        side_range: SSRange,
    ) -> SSRange {
        match ty {
            NodeType::Repeater { .. }
            | NodeType::Torch
            | NodeType::NoteBlock { .. }
            | NodeType::Lamp
            | NodeType::Trapdoor => {
                if matches!(ty, NodeType::Repeater { .. })
                    && state.repeater_locked
                    && side_range.low > 0
                {
                    // This repeater is always locked, use current state
                    return SSRange::constant(state.output_strength);
                }
                // For binary nodes, there are 3 possibilities: always powered, never powered, and
                // sometimes powered
                let always_powered = default_range.low > 0;
                let never_powered = default_range.high == 0;
                if always_powered || never_powered {
                    let output_powered = if *ty == NodeType::Torch {
                        never_powered
                    } else {
                        always_powered
                    };
                    if output_powered {
                        SSRange::constant(15)
                    } else {
                        SSRange::constant(0)
                    }
                } else {
                    SSRange::FULL
                }
            }
            NodeType::Comparator {
                mode, far_input, ..
            } => {
                let input_range = if let Some(far_override) = far_input {
                    if default_range.high < 15 {
                        // The default input can never reach 15 ss, so we always use far override
                        SSRange::constant(*far_override)
                    } else {
                        // The default range reaches 15 ss, overriding the far override, so the high
                        // must be 15. The low is always the far override, because if the default
                        // input is lower than the far override, it cannot possibly be 15.
                        SSRange {
                            low: *far_override,
                            high: 15,
                        }
                    }
                } else {
                    default_range
                };

                match mode {
                    ComparatorMode::Compare => {
                        if input_range.high < side_range.low {
                            // The side input is always greater than the default input
                            SSRange::constant(0)
                        } else if input_range.low >= side_range.high {
                            // The side input is always less than or equal to the default input
                            input_range
                        } else {
                            // The output can be either the default input, or 0 if the side input is
                            // greater
                            let mut range = default_range;
                            range.low = 0;
                            range
                        }
                    }
                    ComparatorMode::Subtract => default_range.saturating_sub(side_range),
                }
            }
            NodeType::Wire => default_range,
            _ => unreachable!("evaluate node ty: {:?}", ty),
        }
    }

    fn range_for_no_inputs(ty: &NodeType, state: &NodeState) -> SSRange {
        match ty {
            NodeType::Repeater { .. }
            | NodeType::Comparator { .. }
            // Nodes that cannot be used as inputs are given 0 arbitrarily
            | NodeType::Lamp
            | NodeType::Trapdoor
            | NodeType::Wire
            | NodeType::NoteBlock { .. } => SSRange::constant(0),
            NodeType::Torch => SSRange::constant(15),
            NodeType::Constant => SSRange::constant(state.output_strength),
            NodeType::Button | NodeType::Lever | NodeType::PressurePlate => SSRange::FULL,
        }
    }
}
