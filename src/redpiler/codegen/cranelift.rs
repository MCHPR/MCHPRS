use super::JITBackend;
use crate::blocks::{self, BlockPos};
use crate::redpiler::Node;
use crate::world::TickEntry;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataContext, Module};

struct CraneliftBackend {
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
    data_ctx: DataContext,
    module: JITModule,
}

impl Default for CraneliftBackend {
    fn default() -> Self {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            data_ctx: DataContext::new(),
            module,
        }
    }
}

impl CraneliftBackend {
    fn translate_node(&mut self, idx: usize, node: &Node) {}
}

impl JITBackend for CraneliftBackend {
    fn compile(&mut self, nodes: Vec<Node>, ticks: Vec<TickEntry>) {
        let mut backend: CraneliftBackend = Default::default();

        for (idx, node) in nodes.iter().enumerate() {
            backend.translate_node(idx, node);
        }

        backend.module.finalize_definitions();
    }

    fn tick(&mut self) {}

    fn on_use_block(&mut self, pos: BlockPos) {}

    fn reset(&mut self) -> Vec<TickEntry> {
        Vec::new()
    }

    fn block_changes(&mut self) -> &mut Vec<(BlockPos, blocks::Block)> {
        unimplemented!();
    }
}
