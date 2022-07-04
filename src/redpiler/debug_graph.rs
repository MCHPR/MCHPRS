use super::CompileNode;
use crate::blocks::Block;
use redpiler_graph::{serialize, BlockPos, ComparatorMode, Link, LinkType, Node, NodeType};
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

convert_enum!(
    crate::blocks::ComparatorMode,
    ComparatorMode,
    Compare,
    Subtract
);

pub fn debug(graph: &[CompileNode]) {
    let mut nodes = Vec::new();
    for node in graph {
        let n = Node {
            ty: match node.state {
                Block::RedstoneRepeater { repeater } => NodeType::Repeater(repeater.delay),
                Block::RedstoneComparator { comparator } => {
                    NodeType::Comparator(comparator.mode.into())
                }
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
            inputs: node
                .inputs
                .iter()
                .map(|l| Link {
                    to: l.end,
                    weight: l.weight,
                })
                .collect(),
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
            pos: BlockPos {
                x: node.pos.x,
                y: node.pos.y,
                z: node.pos.z,
            },
        };
        nodes.push(n);
    }

    fs::write("redpiler_graph.bc", serialize(nodes.as_slice()).unwrap()).unwrap();
}
