use mchprs_blocks::blocks::ComparatorMode;
use mchprs_blocks::BlockPos;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use std::fmt;

pub type NodeIdx = NodeIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Repeater(u8),
    Torch,
    Comparator(ComparatorMode),
    Lamp,
    Button,
    Lever,
    PressurePlate,
    Trapdoor,
    Wire,
    Constant,
    BinBuffer(u8),
    HexBuffer(u8),
}

#[derive(Debug, Clone, Default)]
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

#[derive(Debug)]
pub struct CompileNode {
    pub ty: NodeType,
    pub block: Option<(BlockPos, u32)>,
    pub state: NodeState,

    pub facing_diode: bool,
    pub comparator_far_input: Option<u8>,
    pub is_input: bool,
    pub is_output: bool,
}

impl fmt::Display for CompileNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self.ty {
                NodeType::Repeater(delay) => format!("Repeater({})", delay),
                NodeType::Torch => format!("Torch"),
                NodeType::Comparator(mode) => format!(
                    "Comparator({})",
                    match mode {
                        ComparatorMode::Compare => "Cmp",
                        ComparatorMode::Subtract => "Sub",
                    }
                ),
                NodeType::Lamp => format!("Lamp"),
                NodeType::Button => format!("Button"),
                NodeType::Lever => format!("Lever"),
                NodeType::PressurePlate => format!("PressurePlate"),
                NodeType::Trapdoor => format!("Trapdoor"),
                NodeType::Wire => format!("Wire"),
                NodeType::Constant => format!("Constant"),
                NodeType::BinBuffer(delay) => format!("BinBuffer({})", delay),
                NodeType::HexBuffer(delay) => format!("HexBuffer({})", delay),
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    Default,
    Side,
}

#[derive(Debug)]
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

impl fmt::Display for CompileLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            match self.ty {
                LinkType::Default => "",
                LinkType::Side => "S",
            },
            self.ss
        )
    }
}

pub type CompileGraph = StableGraph<CompileNode, CompileLink>;
