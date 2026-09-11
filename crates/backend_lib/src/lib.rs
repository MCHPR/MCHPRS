use std::sync::Arc;

use mchprs_blocks::blocks::Block;
pub use mchprs_blocks::*;
use mchprs_redpiler::compile_graph::CompileGraph;
pub use mchprs_world::*;
pub use mchprs_redpiler::*;
pub use mchprs_redstone::*;

#[enum_delegate::register]
pub trait JITBackend {
    fn compile(
        &mut self,
        graph: CompileGraph,
        ticks: Vec<TickEntry>,
        options: &CompilerOptions,
        monitor: Arc<TaskMonitor>,
    );
    fn tick(&mut self);

    fn tickn(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn on_use_block(&mut self, pos: BlockPos);
    fn set_pressure_plate(&mut self, pos: BlockPos, powered: bool);
    fn flush<W: World>(&mut self, world: &mut W, io_only: bool);
    fn reset<W: World>(&mut self, world: &mut W, io_only: bool);
    fn has_pending_ticks(&self) -> bool;
    /// Inspect block for debugging
    fn inspect(&mut self, pos: BlockPos);
}

pub fn block_powered_mut(block: &mut Block) -> Option<&mut bool> {
    Some(match block {
        Block::Comparator(comparator) => &mut comparator.powered,
        Block::RedstoneTorch { lit } => lit,
        Block::RedstoneWallTorch { lit, .. } => lit,
        Block::Repeater(repeater) => &mut repeater.powered,
        Block::Lever { powered, .. } => powered,
        Block::StoneButton { powered, .. } => powered,
        Block::RedstoneLamp { lit } => lit,
        Block::IronTrapdoor { powered, .. } => powered,
        Block::NoteBlock { powered, .. } => powered,
        _ => return block.get_pressure_plate_powered(),
    })
}