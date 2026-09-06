//! # [`SSSetAnalysis`]
//!
//! This analysis approximates output strengths using the compile graph's node states and inputs.
//! Every node starts with its current output strength. Sweeps in index order grow each set by what
//! the sets of its inputs allow until a sweep changes nothing. Sets only grow, so the sweeps
//! terminate and can retain tighter bounds through cycles. Correlations between inputs are not
//! tracked.
//!
//! A queued repeater tick can emit a pulse after its input disappears, so pending repeaters
//! also admit both 0 and 15.

use crate::compile_graph::{CompileGraph, Direction, LinkType, NodeIdx, NodeState, NodeType};
use crate::passes::{AnalysisInfo, AnalysisInfos, AnalysisUsage, Pass};
use crate::{CompilerInput, CompilerOptions};
use mchprs_blocks::blocks::ComparatorMode;
use mchprs_world::World;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SSSet(u16);

impl SSSet {
    pub const ZERO: SSSet = SSSet(1);
    pub const BINARY: SSSet = SSSet(1 | 1 << 15);

    pub fn singleton(ss: u8) -> SSSet {
        SSSet(1 << ss)
    }

    fn binary(can_be_off: bool, can_be_on: bool) -> SSSet {
        SSSet(can_be_off as u16 | (can_be_on as u16) << 15)
    }

    pub fn contains(self, ss: u8) -> bool {
        self.0 & (1 << ss) != 0
    }

    pub fn min(self) -> u8 {
        debug_assert!(self.0 != 0);
        self.0.trailing_zeros() as u8
    }

    pub fn max(self) -> u8 {
        debug_assert!(self.0 != 0);
        15 - self.0.leading_zeros() as u8
    }

    pub fn union(self, other: SSSet) -> SSSet {
        SSSet(self.0 | other.0)
    }

    /// The strongest of two inputs is one of the two and at least as strong as both minima
    pub fn combine(self, other: SSSet) -> SSSet {
        let threshold = self.min().max(other.min());
        SSSet((self.0 | other.0) & !((1 << threshold) - 1))
    }

    pub fn decay(self, distance: u8) -> SSSet {
        if distance > 15 {
            return SSSet::ZERO;
        }
        SSSet((self.0 >> distance) | (self.min() < distance) as u16)
    }

    pub fn saturating_sub(self, side: SSSet) -> SSSet {
        (0..16)
            .filter(|&ss| side.contains(ss))
            .map(|ss| self.decay(ss))
            .reduce(SSSet::union)
            .unwrap()
    }

    pub fn saturating_compare(self, side: SSSet) -> SSSet {
        let passing = self.0 & !((1 << side.min()) - 1);
        SSSet(passing | (self.min() < side.max()) as u16)
    }

    pub fn far_override(self, far_input: u8) -> SSSet {
        let far = if self.min() < 15 {
            SSSet::singleton(far_input).0
        } else {
            0
        };
        let full = if self.contains(15) { 1 << 15 } else { 0 };
        SSSet(far | full)
    }

    pub fn to_binary(self) -> SSSet {
        SSSet::binary(self.contains(0), self.max() > 0)
    }

    pub fn to_inverted_binary(self) -> SSSet {
        SSSet::binary(self.max() > 0, self.contains(0))
    }
}

#[derive(Default)]
pub struct SSSetInfo {
    sets: Vec<Option<SSSet>>,
}

impl SSSetInfo {
    pub fn get_set(&self, node_idx: NodeIdx) -> Option<SSSet> {
        self.sets.get(node_idx.index()).copied().flatten()
    }
}

impl AnalysisInfo for SSSetInfo {}

pub struct SSSetAnalysis;

impl<W: World> Pass<W> for SSSetAnalysis {
    fn run_pass(
        &self,
        graph: &mut CompileGraph,
        _: &CompilerOptions,
        _: &CompilerInput<'_, W>,
        analysis_infos: &mut AnalysisInfos,
    ) {
        let nodes: Vec<NodeIdx> = graph.node_indices().collect();
        let mut sets = vec![None; graph.node_bound()];
        let mut dirty = vec![true; graph.node_bound()];
        for &node_idx in &nodes {
            let node = &graph[node_idx];
            let initial = if node.state.pending_tick && matches!(node.ty, NodeType::Repeater { .. })
            {
                SSSet::BINARY
            } else {
                SSSet::singleton(node.state.output_strength)
            };
            sets[node_idx.index()] = Some(initial);
        }

        let mut changed = true;
        while changed {
            changed = false;
            for &node_idx in &nodes {
                if !dirty[node_idx.index()] {
                    continue;
                }
                dirty[node_idx.index()] = false;

                let (default_input, side_input) = collect_inputs(graph, &sets, node_idx);
                let node = &graph[node_idx];
                let old = sets[node_idx.index()].unwrap();
                let new = old.union(evaluate(&node.ty, &node.state, default_input, side_input));
                if new == old {
                    continue;
                }
                sets[node_idx.index()] = Some(new);
                changed = true;

                for neighbor in graph.neighbors(node_idx, Direction::Outgoing) {
                    dirty[neighbor.index()] = true;
                }
            }
        }

        analysis_infos.insert_analysis(SSSetInfo { sets });
    }

    fn status_message(&self) -> &'static str {
        "Analyzing signal strength sets"
    }

    fn analysis_usage(&self, au: &mut AnalysisUsage) {
        au.set_preserves_all();
    }

    fn driver_key(&self) -> &'static str {
        "ss-set-analysis"
    }
}

fn collect_inputs(
    graph: &CompileGraph,
    sets: &[Option<SSSet>],
    node_idx: NodeIdx,
) -> (SSSet, SSSet) {
    let mut default_input = SSSet::ZERO;
    let mut side_input = SSSet::ZERO;
    for edge in graph.edges(node_idx, Direction::Incoming) {
        let link = edge.weight();
        let source = sets[edge.source().index()].unwrap().decay(link.ss);
        match link.ty {
            LinkType::Default => default_input = default_input.combine(source),
            LinkType::Side => side_input = side_input.combine(source),
        }
    }
    (default_input, side_input)
}

fn evaluate(ty: &NodeType, state: &NodeState, default_input: SSSet, side_input: SSSet) -> SSSet {
    match ty {
        NodeType::Repeater { .. } => {
            if side_input.contains(0) {
                default_input.to_binary()
            } else {
                SSSet::singleton(state.output_strength)
            }
        }
        NodeType::Torch => default_input.to_inverted_binary(),
        NodeType::Lamp | NodeType::Trapdoor | NodeType::NoteBlock { .. } => {
            default_input.to_binary()
        }
        NodeType::Comparator {
            mode, far_input, ..
        } => {
            let input = match far_input {
                Some(far_input) => default_input.far_override(*far_input),
                None => default_input,
            };
            match mode {
                ComparatorMode::Compare => input.saturating_compare(side_input),
                ComparatorMode::Subtract => input.saturating_sub(side_input),
            }
        }
        NodeType::Wire => default_input,
        NodeType::Constant => SSSet::singleton(state.output_strength),
        NodeType::Button | NodeType::Lever | NodeType::PressurePlate => SSSet::BINARY,
    }
}
