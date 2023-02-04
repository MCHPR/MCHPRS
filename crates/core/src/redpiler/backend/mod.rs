#[cfg(feature = "jit_cranelift")]
pub mod cranelift;
pub mod direct;
// pub mod par_direct;

use super::compile_graph::CompileGraph;
use crate::world::World;
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;

pub trait JITBackend<W: World> {
    fn compile(&mut self, graph: CompileGraph, ticks: Vec<TickEntry>);
    fn tick(&mut self, world: &mut W);
    fn on_use_block(&mut self, world: &mut W, pos: BlockPos);
    fn set_pressure_plate(&mut self, world: &mut W, pos: BlockPos, powered: bool);
    fn flush(&mut self, world: &mut W, io_only: bool);
    fn reset(&mut self, world: &mut W, io_only: bool);
    /// Inspect block for debugging
    fn inspect(&mut self, pos: BlockPos);
}
