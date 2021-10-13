#[cfg(feature = "jit_cranelift")]
pub mod cranelift;
pub mod direct;
// pub mod par_direct;

use crate::blocks::BlockPos;
use crate::plot::PlotWorld;
use crate::world::TickEntry;

use super::CompileNode;

pub trait JITBackend {
    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>);
    fn tick(&mut self, plot: &mut PlotWorld);
    fn on_use_block(&mut self, plot: &mut PlotWorld, pos: BlockPos);
    fn reset(&mut self, plot: &mut PlotWorld);
}
