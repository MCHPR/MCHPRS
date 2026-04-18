use mchprs_blocks::blocks::{ComparatorMode, Instrument};
use mchprs_blocks::BlockPos;
// use petgraph::stable_graph::{NodeIndex, StableGraph};
use smallvec::SmallVec;
use stable_graph::{NodeIndex, StableGraph};

mod stable_graph;

pub use stable_graph::{Direction, EdgeRef};
pub type NodeIdx = NodeIndex<u32>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeType {
    Repeater {
        delay: u8,
        facing_diode: bool,
    },
    Torch,
    Comparator {
        mode: ComparatorMode,
        far_input: Option<u8>,
        facing_diode: bool,
    },
    Lamp,
    Button,
    Lever,
    PressurePlate,
    Trapdoor,
    Wire,
    Constant,
    NoteBlock {
        instrument: Instrument,
        note: u8,
    },
}

impl NodeType {
    pub fn is_normally_input(&self) -> bool {
        matches!(
            self,
            NodeType::Button | NodeType::Lever | NodeType::PressurePlate
        )
    }

    pub fn is_normally_output(&self) -> bool {
        matches!(
            self,
            NodeType::Trapdoor | NodeType::Lamp | NodeType::NoteBlock { .. }
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeState {
    pub powered: bool,
    pub repeater_locked: bool,
    pub output_strength: u8,
}

impl NodeState {
    pub fn simple(powered: bool) -> NodeState {
        NodeState {
            powered,
            output_strength: if powered { 15 } else { 0 },
            ..Default::default()
        }
    }

    pub fn repeater(powered: bool, locked: bool) -> NodeState {
        NodeState {
            powered,
            repeater_locked: locked,
            output_strength: if powered { 15 } else { 0 },
        }
    }

    pub fn ss(ss: u8) -> NodeState {
        NodeState {
            output_strength: ss,
            ..Default::default()
        }
    }

    pub fn comparator(powered: bool, ss: u8) -> NodeState {
        NodeState {
            powered,
            output_strength: ss,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
pub struct Annotations {}

#[derive(Debug)]
pub struct CompileNode {
    pub ty: NodeType,
    pub block: SmallVec<[(BlockPos, u32); 1]>,
    pub name: Option<String>,
    pub state: NodeState,

    pub is_input: bool,
    pub is_output: bool,
    pub annotations: Annotations,
}

impl CompileNode {
    pub fn is_removable(&self) -> bool {
        !self.is_input && !self.is_output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Default,
    Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileLink {
    pub ty: LinkType,
    pub ss: u8,
}

impl CompileLink {
    pub fn new(ty: LinkType, ss: u8) -> CompileLink {
        CompileLink { ty, ss }
    }

    pub fn default(ss: u8) -> CompileLink {
        CompileLink {
            ty: LinkType::Default,
            ss,
        }
    }

    pub fn side(ss: u8) -> CompileLink {
        CompileLink {
            ty: LinkType::Side,
            ss,
        }
    }
}

pub type CompileGraph = StableGraph<CompileNode, CompileLink, u32>;
