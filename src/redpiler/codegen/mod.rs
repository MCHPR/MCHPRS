pub mod cranelift;
pub mod direct;
pub mod llvm;

use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::world::TickEntry;

use super::Node;

#[derive(Default, Debug)]
pub struct JITResetData {
    pub tick_entries: Vec<TickEntry>,
    pub block_entities: Vec<(BlockPos, BlockEntity)>,
}

pub trait JITBackend {
    fn compile(&mut self, nodes: Vec<Node>, ticks: Vec<TickEntry>);
    fn tick(&mut self);
    fn on_use_block(&mut self, pos: BlockPos);
    fn reset(&mut self) -> JITResetData;
    fn block_changes(&mut self) -> &mut Vec<(BlockPos, Block)>;
}
