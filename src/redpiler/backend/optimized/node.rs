use crate::blocks::{Block, BlockPos, ComparatorMode};
use crate::redpiler::{CompileNode, LinkType, NodeId};

fn get_id_offset(block: Block) -> u32 {
    // Booleans default to true
    match block {
        Block::RedstoneRepeater { mut repeater } => {
            repeater.locked = true;
            repeater.powered = true;
            Block::RedstoneRepeater { repeater }.get_id()
        }
        Block::RedstoneComparator { mut comparator } => {
            comparator.powered = true;
            Block::RedstoneComparator { comparator }.get_id()
        }
        Block::RedstoneTorch { .. } => {
            Block::RedstoneTorch { lit: true }.get_id()
        }
        Block::RedstoneWallTorch { facing, .. } => {
            Block::RedstoneWallTorch { facing, lit: true }.get_id()
        }
        Block::StoneButton { mut button } => {
            button.powered = true;
            Block::StoneButton { button }.get_id()
        }
        Block::StonePressurePlate { .. } => {
            Block::StonePressurePlate { powered: true }.get_id()
        }
        Block::RedstoneLamp { .. } => {
            Block::RedstoneLamp { lit: true }.get_id()
        }
        _ => 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Repeater(u8),
    Comparator(ComparatorMode),
    Torch,
    StoneButton,
    StonePressurePlate,
    Lamp,
    Lever,
    Constant,
}

pub struct WireNode {
    pub id_offset: u32,
    pub pos: BlockPos,
    pub inputs: Vec<Link>,
    pub power: u8,
}

impl WireNode {
    pub fn encode(&self) -> u32 {
        self.id_offset
        + self.power as u32 * 9
    }
}

#[derive(Debug, Clone)]
pub struct Link {
    pub ty: LinkType,
    pub weight: u8,
    pub end: NodeId,
}

impl From<crate::redpiler::Link> for Link {
    fn from(link: crate::redpiler::Link) -> Self {
        Self {
            ty: link.ty,
            weight: link.weight,
            end: link.end,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub ty: NodeType,
    pub id_offset: u32,
    pub pos: BlockPos,
    pub inputs: Vec<Link>,
    pub updates: Vec<usize>,
    pub facing_diode: bool,
    pub comparator_far_input: Option<u8>,

    pub output_power: u8,
    /// Comparator powered / Repeater locked
    pub diode_state: bool,
    pub pending_tick: bool,
}

impl From<CompileNode> for Node {
    fn from(node: CompileNode) -> Self {
        let n = Node {
            ty: match node.state {
                Block::RedstoneRepeater { repeater } => NodeType::Repeater(repeater.delay),
                Block::RedstoneComparator { comparator } => NodeType::Comparator(comparator.mode),
                Block::RedstoneTorch { .. } | Block::RedstoneWallTorch { .. } => NodeType::Torch,
                Block::StoneButton { .. } => NodeType::StoneButton,
                Block::StonePressurePlate { .. } => NodeType::StonePressurePlate,
                Block::RedstoneLamp { .. } => NodeType::Lamp,
                Block::Lever { .. } => NodeType::Lever,
                Block::RedstoneBlock { .. } => NodeType::Constant,
                b if b.has_comparator_override() => NodeType::Constant,
                _ => unreachable!(),
            },
            id_offset: get_id_offset(node.state),
            pos: node.pos,
            inputs: node.inputs.into_iter().map(Into::into).collect(),
            updates: node.updates,
            output_power: match node.state {
                Block::RedstoneRepeater { repeater } => repeater.powered.then(|| 15).unwrap_or(0),
                Block::RedstoneComparator { .. } => node.comparator_output,
                Block::RedstoneTorch { lit } => lit.then(|| 15).unwrap_or(0),
                Block::RedstoneWallTorch { lit, .. } => lit.then(|| 15).unwrap_or(0),
                Block::Lever { lever } => lever.powered.then(|| 15).unwrap_or(0),
                Block::StoneButton { button } => button.powered.then(|| 15).unwrap_or(0),
                Block::RedstoneBlock {} => 15,
                Block::StonePressurePlate { powered } => powered.then(|| 15).unwrap_or(0),
                s if s.has_comparator_override() => node.comparator_output,
                _ => 0,
            },
            diode_state: match node.state {
                Block::RedstoneRepeater { repeater } => repeater.locked,
                Block::RedstoneComparator { comparator } => comparator.powered,
                _ => false,
            },
            facing_diode: node.facing_diode,
            comparator_far_input: node.comparator_far_input,
            pending_tick: false,
        };
        n
    }
}

impl Node {

    // Block encoding functions:
    // Repeater -> encode_repeater
    // Comparator -> encode_generic
    // Torch -> encode_generic
    // Wall Torch -> encode_generic
    // Wire -> encode_wire
    // Stone Button -> encode_generic
    // Stone Pressure Plate -> encode_generic
    // Lamp -> encode_generic
    // Lever -> encode_generic

    pub fn encode_repeater(&self, powered: bool, locked: bool) -> u32 {
        self.id_offset
        + !locked as u32 * 2
        + !powered as u32
    }

    pub fn encode_generic(&self, powered: bool) -> u32 {
        self.id_offset
        + !powered as u32
    }
}
