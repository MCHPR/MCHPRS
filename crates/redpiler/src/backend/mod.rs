pub mod direct;

use std::sync::Arc;

use super::compile_graph::CompileGraph;
use super::task_monitor::TaskMonitor;
use super::CompilerOptions;
use enum_dispatch::enum_dispatch;
use mchprs_blocks::BlockPos;
use mchprs_world::{TickEntry, World};

#[enum_dispatch]
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

use direct::DirectBackend;

#[enum_dispatch(JITBackend)]
#[derive(Clone)]
pub enum BackendDispatcher {
    DirectBackend,
}
