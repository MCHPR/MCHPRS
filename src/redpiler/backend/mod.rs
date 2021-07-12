#[cfg(feature = "jit_cranelift")]
pub mod cranelift;
pub mod direct;
pub mod par_direct;

use crate::blocks::{Block, BlockEntity, BlockPos};
use crate::world::TickEntry;

use super::CompileNode;

#[derive(Default, Debug)]
pub struct JITResetData {
    pub tick_entries: Vec<TickEntry>,
    pub block_entities: Vec<(BlockPos, BlockEntity)>,
}

pub trait JITBackend {
    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>);
    fn tick(&mut self);
    fn on_use_block(&mut self, pos: BlockPos);
    fn reset(&mut self) -> JITResetData;
    fn block_changes(&mut self) -> &mut Vec<(BlockPos, Block)>;
}
