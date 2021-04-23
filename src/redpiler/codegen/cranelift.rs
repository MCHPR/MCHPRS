use super::JITBackend;
use crate::blocks::{self, Block, BlockPos};
use crate::redpiler::Node;
use crate::world::{TickEntry, TickPriority};
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataContext, Linkage, Module};
use log::warn;
use std::collections::HashMap;

struct CLTickEntry {
    ticks_left: u32,
    priority: TickPriority,
    tick_fn: extern "C" fn(&mut CraneliftBackend),
}

struct CraneliftBackend {
    // Compilation
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
    module: JITModule,
    // Execution
    initial_nodes: Vec<Node>,
    tick_fns: Vec<extern "C" fn(&mut CraneliftBackend)>,
    use_fns: Vec<extern "C" fn(&mut CraneliftBackend)>,
    pos_map: HashMap<BlockPos, usize>,
    to_be_ticked: Vec<CLTickEntry>,
    change_queue: Vec<(BlockPos, Block)>,
}

impl Default for CraneliftBackend {
    fn default() -> Self {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            module,
            ..Default::default()
        }
    }
}

impl CraneliftBackend {
    fn translate_comparator_tick(&mut self, idx: usize, node: &Node, nodes: &[Node]) {}

    fn translate_comparator_update(&mut self, idx: usize, node: &Node, nodes: &[Node]) {}

    fn translate_node(&mut self, idx: usize, node: &Node, nodes: &[Node]) {
        match node.state {
            Block::RedstoneComparator { .. } => {
                self.translate_comparator_update(idx, node, nodes);
                self.translate_comparator_tick(idx, node, nodes);
            }
            Block::RedstoneTorch { .. } => {}
            Block::RedstoneWallTorch { .. } => {}
            Block::RedstoneRepeater { .. } => {}
            Block::RedstoneWire { .. } => {}
            Block::Lever { .. } => {}
            Block::StoneButton { .. } => {}
            Block::RedstoneBlock { .. } => {}
            Block::RedstoneLamp { .. } => {}
            state => warn!("Trying to compile node with state {:?}", state),
        }
    }
}

impl JITBackend for CraneliftBackend {
    fn compile(&mut self, nodes: Vec<Node>, ticks: Vec<TickEntry>) {
        let mut data_ctx = DataContext::new();
        for idx in 0..nodes.len() {
            let output_power_name = format!("n{}_output_power", idx);
            let comparator_output_name = format!("n{}_output_power", idx);

            data_ctx.define_zeroinit(1);
            let output_power_id = self
                .module
                .declare_data(&output_power_name, Linkage::Local, true, false)
                .unwrap();
            self.module.define_data(output_power_id, &data_ctx).unwrap();
            data_ctx.clear();

            data_ctx.define_zeroinit(1);
            let comparator_output_id = self
                .module
                .declare_data(&comparator_output_name, Linkage::Local, true, false)
                .unwrap();
            self.module
                .define_data(comparator_output_id, &data_ctx).unwrap();
            data_ctx.clear();
        }

        for (idx, node) in nodes.iter().enumerate() {
            self.translate_node(idx, node, &nodes);
        }

        self.module.finalize_definitions();

        for (i, node) in nodes.iter().enumerate() {
            self.pos_map.insert(node.pos, i);
        }

        for entry in ticks {
            self.to_be_ticked.push(CLTickEntry {
                ticks_left: entry.ticks_left,
                priority: entry.tick_priority,
                tick_fn: self.tick_fns[self.pos_map[&entry.pos]],
            })
        }

        self.initial_nodes = nodes;
    }

    fn tick(&mut self) {
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.priority));
        while self.to_be_ticked.first().map(|e| e.ticks_left).unwrap_or(1) == 0 {
            let entry = self.to_be_ticked.remove(0);
            (entry.tick_fn)(self);
        }
    }

    fn on_use_block(&mut self, pos: BlockPos) {
        self.use_fns[self.pos_map[&pos]](self);
    }

    fn reset(&mut self) -> Vec<TickEntry> {
        let mut ticks = Vec::new();
        for entry in self.to_be_ticked.drain(..) {
            ticks.push(TickEntry {
                ticks_left: entry.ticks_left,
                tick_priority: entry.priority,
                pos: self.initial_nodes[self
                    .tick_fns
                    .iter()
                    .position(|f| *f as usize == entry.tick_fn as usize)
                    .unwrap()]
                .pos,
            })
        }
        ticks
    }

    fn block_changes(&mut self) -> &mut Vec<(BlockPos, blocks::Block)> {
        &mut self.change_queue
    }
}

#[no_mangle]
extern "C" fn cranelift_jit_schedule_tick(
    backend: &mut CraneliftBackend,
    delay: u32,
    priority: u8,
    tick_fn: extern "C" fn(&mut CraneliftBackend),
) {
    backend.to_be_ticked.push(CLTickEntry {
        ticks_left: delay,
        priority: match priority {
            0 => TickPriority::Normal,
            1 => TickPriority::High,
            2 => TickPriority::Higher,
            3 => TickPriority::Highest,
            _ => panic!("Cranelift JIT scheduled tick with priority of {}", priority),
        },
        tick_fn,
    })
}
