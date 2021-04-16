pub mod cranelift;
pub mod direct;
pub mod llvm;

use crate::blocks::{Block, BlockPos};
use crate::world::TickEntry;

use super::Node;

pub trait JITBackend {
    fn compile(&mut self, nodes: Vec<Node>, ticks: Vec<TickEntry>);
    fn tick(&mut self);
    fn on_use_block(&mut self, pos: BlockPos);
    fn reset(&mut self) -> Vec<TickEntry>;
    fn block_changes(&mut self) -> &mut Vec<(BlockPos, Block)>;
}
