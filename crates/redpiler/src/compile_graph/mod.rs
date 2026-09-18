mod stable_graph;

use mchprs_blocks::{
    blocks::{ComparatorMode, Instrument},
    BlockPos,
};
use smallvec::SmallVec;

use self::stable_graph::{NodeIndex, StableGraph};

pub use self::stable_graph::{Direction, EdgeRef};
pub use redpiler_graph::SignalStrength;
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
        far_input: Option<SignalStrength>,
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
    pub power: SignalStrength,
    pub repeater_locked: bool,
}

impl NodeState {
    pub fn from_power(power: SignalStrength) -> Self {
        Self {
            power,
            ..Default::default()
        }
    }

    pub fn from_powered(powered: bool) -> Self {
        Self::from_power(powered.into())
    }

    pub fn repeater(powered: bool, locked: bool) -> Self {
        Self {
            power: powered.into(),
            repeater_locked: locked,
        }
    }

    pub fn is_powered(&self) -> bool {
        !self.power.is_zero()
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
    pub weight: u8,
}

impl CompileLink {
    pub fn new(ty: LinkType, weight: u8) -> Self {
        Self { ty, weight }
    }

    pub fn default(weight: u8) -> Self {
        Self {
            ty: LinkType::Default,
            weight,
        }
    }

    pub fn side(weight: u8) -> Self {
        Self {
            ty: LinkType::Side,
            weight,
        }
    }
}

pub type CompileGraph = StableGraph<CompileNode, CompileLink, u32>;
