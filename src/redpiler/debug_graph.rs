use super::{CompileNode, NodeId};
use crate::blocks::{Block, BlockPos};
use serde::Serialize;
use std::fs;

macro_rules! convert_enum {
    ($src:path, $dst:ident, $($variant:ident),*) => {
        impl From<$src> for $dst {
            fn from(src: $src) -> Self {
                match src {
                    $(<$src>::$variant => Self::$variant,)*
                }
            }
        }
    }
}

#[derive(Serialize)]
enum LinkType {
    Default,
    Side,
}

convert_enum!(super::LinkType, LinkType, Default, Side);

#[derive(Serialize)]
enum ComparatorMode {
    Compare,
    Subtract,
}

convert_enum!(crate::blocks::ComparatorMode, ComparatorMode, Compare, Subtract);

#[derive(Serialize)]
struct Link {
    pub ty: LinkType,
    pub weight: u8,
    pub to: NodeId,
}

#[derive(Serialize)]
enum NodeType {
    Repeater(u8),
    Comparator(ComparatorMode),
    Torch,
    StoneButton,
    StonePressurePlate,
    Lamp,
    Lever,
    Constant,
    Wire,
}

#[derive(Serialize)]
struct Node {
    pub ty: NodeType,
    pub inputs: Vec<Link>,
    pub updates: Vec<NodeId>,
    pub facing_diode: bool,
    pub comparator_far_input: Option<u8>,
    pub output_power: u8,
    /// Comparator powered / Repeater locked
    pub diode_state: bool,
    pub pos: BlockPos,
}

pub fn debug(graph: &[CompileNode]) {
    let mut nodes = Vec::new();
    for node in graph {
        let n = Node {
            ty: match node.state {
                Block::RedstoneRepeater { repeater } => NodeType::Repeater(repeater.delay),
                Block::RedstoneComparator { comparator } => NodeType::Comparator(comparator.mode.into()),
                Block::RedstoneTorch { .. } => NodeType::Torch,
                Block::RedstoneWallTorch { .. } => NodeType::Torch,
                Block::StoneButton { .. } => NodeType::StoneButton,
                Block::StonePressurePlate { .. } => NodeType::StonePressurePlate,
                Block::RedstoneLamp { .. } => NodeType::Lamp,
                Block::Lever { .. } => NodeType::Lever,
                Block::RedstoneBlock { .. } => NodeType::Constant,
                Block::RedstoneWire { .. } => NodeType::Wire,
                block if block.has_comparator_override() => NodeType::Constant,
                _ => continue,
            },
            inputs: node.inputs.iter().map(|l| Link {
                ty: l.ty.into(),
                to: l.end,
                weight: l.weight,
            }).collect(),
            updates: node.updates.clone(),
            comparator_far_input: node.comparator_far_input,
            diode_state: match node.state {
                Block::RedstoneRepeater { repeater } => repeater.locked,
                Block::RedstoneComparator { comparator } => comparator.powered,
                _ => false,
            },
            facing_diode: node.facing_diode,
            output_power: match node.state {
                Block::RedstoneRepeater { repeater } => repeater.powered.then(|| 15).unwrap_or(0),
                Block::RedstoneComparator { .. } => node.comparator_output,
                Block::RedstoneTorch { lit } => lit.then(|| 15).unwrap_or(0),
                Block::RedstoneWallTorch { lit, .. } => lit.then(|| 15).unwrap_or(0),
                Block::Lever { lever } => lever.powered.then(|| 15).unwrap_or(0),
                Block::StoneButton { button } => button.powered.then(|| 15).unwrap_or(0),
                Block::StonePressurePlate { powered } => powered.then(|| 15).unwrap_or(0),
                Block::RedstoneBlock {} => 15,
                s if s.has_comparator_override() => node.comparator_output,
                _ => 0,
            },
            pos: node.pos,
        };
        nodes.push(n);
    }
    
    fs::write("redpiler_graph.bc", bincode::serialize(&nodes).unwrap()).unwrap();
}
