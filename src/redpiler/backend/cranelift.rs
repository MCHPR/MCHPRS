use super::{JITBackend, JITResetData};
use crate::blocks::{
    self, Block, BlockPos, ComparatorMode, Lever, RedstoneComparator, RedstoneRepeater,
};
use crate::redpiler::{CompileNode, Link, LinkType};
use crate::world::{TickEntry, TickPriority};
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataContext, DataId, FuncId, Linkage, Module};
use log::debug;
use std::collections::HashMap;

struct CLTickEntry {
    ticks_left: u32,
    priority: TickPriority,
    node_id: usize,
}

struct FunctionTranslator<'a> {
    builder: FunctionBuilder<'a>,
    module: &'a mut JITModule,
    output_power_data: &'a [DataId],
    comparator_output_data: &'a [DataId],
    repeater_lock_data: &'a [DataId],
    node_idx: usize,
    node: &'a CompileNode,
    nodes: &'a [CompileNode],
}

impl<'a> FunctionTranslator<'a> {
    #[allow(dead_code)]
    fn call_debug_val(&mut self, val: Value) {
        let mut sig = self.module.make_signature();
        sig.params = vec![AbiParam::new(types::I32)];

        let callee = self
            .module
            .declare_function("cranelift_jit_debug_val", Linkage::Import, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);

        self.builder.ins().call(local_callee, &[val]);
    }

    fn call_schedule_tick(
        &mut self,
        backend: Value,
        node_id: usize,
        delay: u32,
        priority: CLTickPriority,
    ) {
        let mut sig = self.module.make_signature();
        let pointer_type = self.module.target_config().pointer_type();
        sig.params = vec![
            AbiParam::new(pointer_type),
            AbiParam::new(pointer_type),
            AbiParam::new(types::I32),
            AbiParam::new(types::I8),
        ];

        let callee = self
            .module
            .declare_function("cranelift_jit_schedule_tick", Linkage::Import, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);

        let node_id = self.builder.ins().iconst(pointer_type, node_id as i64);
        let delay = self.builder.ins().iconst(types::I32, delay as i64);
        let priority = self.builder.ins().iconst(types::I8, priority as i64);

        self.builder
            .ins()
            .call(local_callee, &[backend, node_id, delay, priority]);
    }

    /// Returns b1 value
    fn call_pending_tick_at(&mut self, backend: Value, node_id: usize) -> Value {
        let mut sig = self.module.make_signature();
        let pointer_type = self.module.target_config().pointer_type();
        sig.params = vec![AbiParam::new(pointer_type), AbiParam::new(pointer_type)];

        sig.returns.push(AbiParam::new(types::B1));

        let callee = self
            .module
            .declare_function("cranelift_jit_pending_tick_at", Linkage::Import, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);

        let node_id = self.builder.ins().iconst(pointer_type, node_id as i64);

        let call = self.builder.ins().call(local_callee, &[backend, node_id]);
        self.builder.inst_results(call)[0]
    }

    fn call_update_node(&mut self, backend: Value, node_id: usize) {
        let mut sig = self.module.make_signature();
        let pointer_type = self.module.target_config().pointer_type();
        sig.params = vec![AbiParam::new(pointer_type)];
        let callee = self
            .module
            .declare_function(&format!("n{}_update", node_id), Linkage::Export, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);
        self.builder.ins().call(local_callee, &[backend]);
    }

    fn call_set_node(&mut self, backend: Value, node_id: usize, power: Value, update: bool) {
        let mut sig = self.module.make_signature();
        let pointer_type = self.module.target_config().pointer_type();
        sig.params = vec![
            AbiParam::new(pointer_type),
            AbiParam::new(pointer_type),
            AbiParam::new(types::I32),
        ];

        let callee = self
            .module
            .declare_function("cranelift_jit_set_node", Linkage::Import, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);

        let node_id_v = self.builder.ins().iconst(pointer_type, node_id as i64);

        self.builder
            .ins()
            .call(local_callee, &[backend, node_id_v, power]);

        if update {
            for update in self.node.updates.clone() {
                self.call_update_node(backend, update.index);
            }
            self.call_update_node(backend, node_id);
        }
    }

    fn call_set_locked(&mut self, backend: Value, node_id: usize, val: bool) {
        let mut sig = self.module.make_signature();
        let pointer_type = self.module.target_config().pointer_type();
        sig.params = vec![
            AbiParam::new(pointer_type),
            AbiParam::new(pointer_type),
            AbiParam::new(types::B8),
        ];

        let callee = self
            .module
            .declare_function("cranelift_jit_set_locked", Linkage::Import, &sig)
            .unwrap();
        let local_callee = self
            .module
            .declare_func_in_func(callee, &mut self.builder.func);

        let node_id_v = self.builder.ins().iconst(pointer_type, node_id as i64);
        let val_v = self.builder.ins().bconst(types::B8, val);

        self.builder
            .ins()
            .call(local_callee, &[backend, node_id_v, val_v]);
    }

    fn translate_max(&mut self, a: Value, b: Value) -> Value {
        let c = self
            .builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, a, b);
        self.builder.ins().select(c, a, b)
    }

    // This is needed because the bnot instruction is unimplemented
    fn translate_bnot(&mut self, a: Value) -> Value {
        let int = self.builder.ins().bint(types::I8, a);
        self.builder.ins().icmp_imm(IntCC::Equal, int, 0)
    }

    // This is needed because the band_not instruction is unimplemented
    fn translate_band_not(&mut self, a: Value, b: Value) -> Value {
        let not_b = self.translate_bnot(b);
        self.builder.ins().band(a, not_b)
    }

    // This is needed because the band_not instruction is unimplemented
    fn translate_bxor_not(&mut self, a: Value, b: Value) -> Value {
        let not_b = self.translate_bnot(b);
        self.builder.ins().bxor(a, not_b)
    }

    fn get_data(&mut self, data: DataId) -> Value {
        let gv = self
            .module
            .declare_data_in_func(data, &mut self.builder.func);
        let p = self
            .builder
            .ins()
            .symbol_value(self.module.target_config().pointer_type(), gv);
        let i8 = self.builder.ins().load(types::I8, MemFlags::new(), p, 0);
        self.builder.ins().uextend(types::I32, i8)
    }

    fn set_data(&mut self, data: DataId, val: Value) {
        let gv = self
            .module
            .declare_data_in_func(data, &mut self.builder.func);
        let p = self
            .builder
            .ins()
            .symbol_value(self.module.target_config().pointer_type(), gv);
        self.builder.ins().istore8(MemFlags::new(), val, p, 0);
    }

    fn set_data_imm(&mut self, data: DataId, val: i64) -> Value {
        let gv = self
            .module
            .declare_data_in_func(data, &mut self.builder.func);
        let p = self
            .builder
            .ins()
            .symbol_value(self.module.target_config().pointer_type(), gv);
        let imm = self.builder.ins().iconst(types::I8, val);
        self.builder.ins().store(MemFlags::new(), imm, p, 0);
        imm
    }

    fn translate_output_power(&mut self, idx: usize) -> Value {
        let node = &self.nodes[idx];
        let gv = if matches!(node.state, Block::RedstoneComparator { .. })
            || node.state.has_comparator_override()
        {
            self.module
                .declare_data_in_func(self.comparator_output_data[idx], &mut self.builder.func)
        } else {
            self.module
                .declare_data_in_func(self.output_power_data[idx], &mut self.builder.func)
        };
        let p = self
            .builder
            .ins()
            .symbol_value(self.module.target_config().pointer_type(), gv);
        let i8 = self.builder.ins().load(types::I8, MemFlags::new(), p, 0);
        self.builder.ins().uextend(types::I32, i8)
    }

    /// Recursive method that returns (input_power, side_input_power)
    fn translate_node_input_power_recur(
        &mut self,
        inputs: &[Link],
        input_power: Value,
        side_input_power: Value,
    ) -> (Value, Value) {
        let zero = self.builder.ins().iconst(types::I32, 0);
        match inputs.first() {
            Some(input) => match input.ty {
                LinkType::Default => {
                    let v = self.translate_output_power(input.end.index);
                    let weight = self.builder.ins().iconst(types::I32, input.weight as i64);
                    let weighted = self.builder.ins().isub(v, weight);
                    let weighted_sat = self.translate_max(weighted, zero);
                    let new_input_power = self.translate_max(weighted_sat, input_power);
                    self.translate_node_input_power_recur(
                        &inputs[1..],
                        new_input_power,
                        side_input_power,
                    )
                }
                LinkType::Side => {
                    let v = self.translate_output_power(input.end.index);
                    let weight = self.builder.ins().iconst(types::I32, input.weight as i64);
                    let weighted = self.builder.ins().isub(v, weight);
                    let weighted_sat = self.translate_max(weighted, zero);
                    let new_side_input_power = self.translate_max(weighted_sat, side_input_power);
                    self.translate_node_input_power_recur(
                        &inputs[1..],
                        input_power,
                        new_side_input_power,
                    )
                }
            },
            None => (input_power, side_input_power),
        }
    }

    /// returns (input_power, side_input_power)
    fn translate_node_input_power(&mut self, inputs: &[Link]) -> (Value, Value) {
        let input_power = self.builder.ins().iconst(types::I32, 0);
        let side_input_power = self.builder.ins().iconst(types::I32, 0);
        self.translate_node_input_power_recur(inputs, input_power, side_input_power)
    }

    fn translate_update(&mut self, entry_block: cranelift::prelude::Block) {
        let backend = self.builder.block_params(entry_block)[0];
        match self.node.state {
            Block::RedstoneComparator { comparator } => {
                self.translate_comparator_update(backend, comparator)
            }
            Block::RedstoneTorch { .. } => self.translate_redstone_torch_update(backend),
            Block::RedstoneWallTorch { .. } => self.translate_redstone_torch_update(backend),
            Block::RedstoneRepeater { repeater } => {
                self.translate_redstone_repeater_update(backend, repeater)
            }
            Block::RedstoneWire { .. } => self.translate_redstone_wire_update(backend),
            Block::Lever { .. } => {}
            Block::StoneButton { .. } => {}
            Block::RedstoneBlock { .. } => {}
            Block::RedstoneLamp { .. } => self.translate_redstone_lamp_update(backend),
            _ => {} // state => warn!("Trying to compile node with state {:?}", state),
        }
        self.builder.ins().return_(&[]);
    }

    fn translate_tick(&mut self, entry_block: cranelift::prelude::Block) {
        let backend = self.builder.block_params(entry_block)[0];
        match self.node.state {
            Block::RedstoneComparator { comparator } => {
                self.translate_comparator_tick(backend, comparator)
            }
            Block::RedstoneTorch { .. } => self.translate_redstone_torch_tick(backend),
            Block::RedstoneWallTorch { .. } => self.translate_redstone_torch_tick(backend),
            Block::RedstoneRepeater { .. } => self.translate_redstone_repeater_tick(backend),
            Block::RedstoneWire { .. } => {}
            Block::Lever { .. } => {}
            Block::StoneButton { .. } => {}
            Block::RedstoneBlock { .. } => {}
            Block::RedstoneLamp { .. } => self.translate_redstone_lamp_tick(backend),
            _ => {} // state => warn!("Trying to compile node with state {:?}", state),
        }
        self.builder.ins().return_(&[]);
    }

    fn translate_calculate_comparator_output(
        &mut self,
        mode: ComparatorMode,
        input_strength: Value,
        power_on_sides: Value,
    ) -> Value {
        if mode == ComparatorMode::Subtract {
            let z = self.builder.ins().iconst(types::I32, 0);
            let output = self.builder.ins().isub(input_strength, power_on_sides);
            let output_sat = self.translate_max(output, z);
            return output_sat;
        }

        let z = self.builder.ins().iconst(types::I32, 0);
        let c = self.builder.ins().icmp(
            IntCC::UnsignedGreaterThanOrEqual,
            input_strength,
            power_on_sides,
        );
        self.builder.ins().select(c, input_strength, z)
    }

    fn translate_comparator_should_be_powered(
        &mut self,
        mode: ComparatorMode,
        input_strength: Value,
        power_on_sides: Value,
    ) -> Value {
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::B1);

        let true_val = self.builder.ins().bconst(types::B1, true);
        let false_val = self.builder.ins().bconst(types::B1, false);
        self.builder
            .ins()
            .brz(input_strength, merge_block, &[false_val]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let cnd1 =
            self.builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThan, input_strength, power_on_sides);
        let cnd = if mode == ComparatorMode::Compare {
            let cnd2 = self
                .builder
                .ins()
                .icmp(IntCC::Equal, input_strength, power_on_sides);
            self.builder.ins().bor(cnd1, cnd2)
        } else {
            cnd1
        };
        self.builder.ins().brnz(cnd, merge_block, &[true_val]);
        self.builder.ins().jump(merge_block, &[false_val]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        self.builder.block_params(merge_block)[0]
    }

    fn translate_comparator_update(&mut self, backend: Value, comparator: RedstoneComparator) {
        let (input_power, side_input_power) = self.translate_node_input_power(&self.node.inputs);
        let return_block = self.builder.create_block();

        let pending_tick = self.call_pending_tick_at(backend, self.node_idx);
        let main_block = self.builder.create_block();
        self.builder.ins().brz(pending_tick, main_block, &[]);
        self.builder.ins().jump(return_block, &[]);
        self.builder.switch_to_block(main_block);
        self.builder.seal_block(main_block);

        let output_power = self.translate_calculate_comparator_output(
            comparator.mode,
            input_power,
            side_input_power,
        );
        let old_strength = self.get_data(self.comparator_output_data[self.node_idx]);

        let schedule_block = self.builder.create_block();
        let powered_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let powered = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, powered_i32, 0);
        let should_be_powered = self.translate_comparator_should_be_powered(
            comparator.mode,
            input_power,
            side_input_power,
        );
        let cnd1 = self
            .builder
            .ins()
            .icmp(IntCC::NotEqual, output_power, old_strength);
        let cnd2 = self.builder.ins().bxor(powered, should_be_powered); // boolean not equals
        let cnd_or = self.builder.ins().bor(cnd1, cnd2);
        self.builder.ins().brnz(cnd_or, schedule_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(schedule_block);
        self.builder.seal_block(schedule_block);
        let priority = if self.node.facing_diode {
            CLTickPriority::High
        } else {
            CLTickPriority::Normal
        };
        self.call_schedule_tick(backend, self.node_idx, 1, priority);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_comparator_tick(&mut self, backend: Value, comparator: RedstoneComparator) {
        let return_block = self.builder.create_block();
        let (input_power, side_input_power) = self.translate_node_input_power(&self.node.inputs);

        let new_strength = self.translate_calculate_comparator_output(
            comparator.mode,
            input_power,
            side_input_power,
        );
        let old_strength = self.get_data(self.comparator_output_data[self.node_idx]);
        // self.call_debug_val(new_strength);
        // self.call_debug_val(old_strength);
        if comparator.mode != ComparatorMode::Compare {
            let change_block = self.builder.create_block();
            self.builder.ins().br_icmp(
                IntCC::NotEqual,
                new_strength,
                old_strength,
                change_block,
                &[],
            );
            self.builder.ins().jump(return_block, &[]);

            self.builder.switch_to_block(change_block);
            self.builder.seal_block(change_block);
        }

        self.set_data(self.comparator_output_data[self.node_idx], new_strength);
        let should_be_powered = self.translate_comparator_should_be_powered(
            comparator.mode,
            input_power,
            side_input_power,
        );
        let powered_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let powered = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, powered_i32, 0);

        let set_powered_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let set_not_powered_block = self.builder.create_block();
        let set_node_block = self.builder.create_block();
        self.builder.append_block_param(set_node_block, types::I32);

        let should_set_powered = self.translate_band_not(should_be_powered, powered);
        self.builder
            .ins()
            .brnz(should_set_powered, set_powered_block, &[]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let should_set_not_powered = self.translate_band_not(powered, should_be_powered);
        self.builder
            .ins()
            .brnz(should_set_not_powered, set_not_powered_block, &[]);
        self.builder.ins().jump(set_node_block, &[powered_i32]);

        self.builder.switch_to_block(set_powered_block);
        self.builder.seal_block(set_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 15);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.builder
            .ins()
            .jump(set_node_block, &[new_output_power_i32]);

        self.builder.switch_to_block(set_not_powered_block);
        self.builder.seal_block(set_not_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 0);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.builder
            .ins()
            .jump(set_node_block, &[new_output_power_i32]);

        self.builder.switch_to_block(set_node_block);
        self.builder.seal_block(set_node_block);
        let new_output_power_i32 = self.builder.block_params(set_node_block)[0];
        self.call_set_node(backend, self.node_idx, new_output_power_i32, true);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_repeater_update(&mut self, backend: Value, repeater: RedstoneRepeater) {
        let return_block = self.builder.create_block();
        let main_block = self.builder.create_block();
        self.builder.append_block_param(main_block, types::I32); // Locked
        let (input_power, side_input_power) = self.translate_node_input_power(&self.node.inputs);

        let should_be_locked =
            self.builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThan, side_input_power, 0);
        let locked_i32 = self.get_data(self.repeater_lock_data[self.node_idx]);
        let locked = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, locked_i32, 0);

        let set_locked_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let set_not_locked_block = self.builder.create_block();

        let should_set_locked = self.translate_band_not(should_be_locked, locked);
        self.builder
            .ins()
            .brnz(should_set_locked, set_locked_block, &[]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let should_set_not_powered = self.translate_band_not(locked, should_be_locked);
        self.builder
            .ins()
            .brnz(should_set_not_powered, set_not_locked_block, &[]);
        self.builder.ins().jump(main_block, &[locked_i32]);

        self.builder.switch_to_block(set_locked_block);
        self.builder.seal_block(set_locked_block);
        let new_locked = self.set_data_imm(self.repeater_lock_data[self.node_idx], 1);
        let new_locked_i32 = self.builder.ins().uextend(types::I32, new_locked);
        self.call_set_locked(backend, self.node_idx, true);
        self.builder.ins().jump(main_block, &[new_locked_i32]);

        self.builder.switch_to_block(set_not_locked_block);
        self.builder.seal_block(set_not_locked_block);
        let new_locked = self.set_data_imm(self.repeater_lock_data[self.node_idx], 0);
        let new_locked_i32 = self.builder.ins().uextend(types::I32, new_locked);
        self.call_set_locked(backend, self.node_idx, false);
        self.builder.ins().jump(main_block, &[new_locked_i32]);

        self.builder.switch_to_block(main_block);
        self.builder.seal_block(main_block);

        // condition 1: !locked && !pending_tick_at(self)
        let locked_i32 = self.builder.block_params(main_block)[0];
        let locked = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, locked_i32, 0);
        let not_locked = self.translate_bnot(locked);
        let pending_tick_at = self.call_pending_tick_at(backend, self.node_idx);
        let not_pending_tick_at = self.translate_bnot(pending_tick_at);
        let cond1 = self.builder.ins().band(not_locked, not_pending_tick_at);

        // condition 2: should_be_powered != powered
        let should_be_powered =
            self.builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);
        let powered_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let powered = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, powered_i32, 0);
        let cond2 = self.builder.ins().bxor(should_be_powered, powered);

        let cond = self.builder.ins().band(cond1, cond2);
        let schedule_tick_block = self.builder.create_block();
        self.builder.ins().brnz(cond, schedule_tick_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(schedule_tick_block);
        self.builder.seal_block(schedule_tick_block);
        if self.node.facing_diode {
            self.call_schedule_tick(
                backend,
                self.node_idx,
                repeater.delay as u32,
                CLTickPriority::Highest,
            );
        } else {
            let schedule_higher_block = self.builder.create_block();
            let schedule_high_block = self.builder.create_block();
            // if !should_be_powered { TickPriority::Higher }
            self.builder
                .ins()
                .brz(should_be_powered, schedule_higher_block, &[]);
            // else { TickPriority::High }
            self.builder.ins().jump(schedule_high_block, &[]);
            self.builder.switch_to_block(schedule_high_block);
            self.builder.seal_block(schedule_high_block);
            self.call_schedule_tick(
                backend,
                self.node_idx,
                repeater.delay as u32,
                CLTickPriority::High,
            );
            self.builder.ins().jump(return_block, &[]);
            self.builder.switch_to_block(schedule_higher_block);
            self.builder.seal_block(schedule_higher_block);
            self.call_schedule_tick(
                backend,
                self.node_idx,
                repeater.delay as u32,
                CLTickPriority::Higher,
            );
        }
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_repeater_tick(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);

        let main_block = self.builder.create_block();
        let locked = self.get_data(self.repeater_lock_data[self.node_idx]);
        self.builder.ins().brnz(locked, return_block, &[]);
        self.builder.ins().jump(main_block, &[]);

        self.builder.switch_to_block(main_block);
        self.builder.seal_block(main_block);
        let should_be_powered =
            self.builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);
        let powered_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let powered = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, powered_i32, 0);

        let set_powered_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let set_not_powered_block = self.builder.create_block();

        let should_set_powered = self.translate_band_not(should_be_powered, powered);
        self.builder
            .ins()
            .brnz(should_set_powered, set_powered_block, &[]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let should_set_not_powered = self.translate_band_not(powered, should_be_powered);
        self.builder
            .ins()
            .brnz(should_set_not_powered, set_not_powered_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_powered_block);
        self.builder.seal_block(set_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 15);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, true);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_not_powered_block);
        self.builder.seal_block(set_not_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 0);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, true);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_torch_update(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);
        let should_be_off = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);

        let lit_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, lit_i32, 0);

        let pending_tick_at = self.call_pending_tick_at(backend, self.node_idx);
        let not_pending_tick_at = self.translate_bnot(pending_tick_at);

        let cond1 = self.translate_bxor_not(lit, should_be_off);
        let cond = self.builder.ins().band(cond1, not_pending_tick_at);
        let schedule_tick_block = self.builder.create_block();
        self.builder.ins().brnz(cond, schedule_tick_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(schedule_tick_block);
        self.builder.seal_block(schedule_tick_block);
        self.call_schedule_tick(backend, self.node_idx, 1, CLTickPriority::Normal);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_torch_tick(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);
        let should_be_off = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);

        let lit_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, lit_i32, 0);

        let set_powered_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let set_not_powered_block = self.builder.create_block();

        let not_should_set_powered = self.builder.ins().bor(should_be_off, lit);
        let should_set_powered = self.translate_bnot(not_should_set_powered);
        self.builder
            .ins()
            .brnz(should_set_powered, set_powered_block, &[]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let should_set_not_powered = self.builder.ins().band(lit, should_be_off);
        self.builder
            .ins()
            .brnz(should_set_not_powered, set_not_powered_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_powered_block);
        self.builder.seal_block(set_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 15);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, true);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_not_powered_block);
        self.builder.seal_block(set_not_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 0);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, true);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_lamp_update(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);
        let should_be_lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);

        let lit_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, lit_i32, 0);

        let set_powered_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let set_not_powered_block = self.builder.create_block();

        let should_set_powered = self.translate_band_not(should_be_lit, lit);
        self.builder
            .ins()
            .brnz(should_set_powered, set_powered_block, &[]);
        self.builder.ins().jump(else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let should_set_not_powered = self.translate_band_not(lit, should_be_lit);
        self.builder
            .ins()
            .brnz(should_set_not_powered, set_not_powered_block, &[]);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_powered_block);
        self.builder.seal_block(set_powered_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 15);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, false);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_not_powered_block);
        self.builder.seal_block(set_not_powered_block);
        self.call_schedule_tick(backend, self.node_idx, 2, CLTickPriority::Normal);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_lamp_tick(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);
        let should_be_lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, input_power, 0);

        let lit_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let lit = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, lit_i32, 0);

        let should_set_not_lit = self.translate_band_not(lit, should_be_lit);
        let set_not_lit_block = self.builder.create_block();
        self.builder
            .ins()
            .brz(should_set_not_lit, return_block, &[]);
        self.builder.ins().jump(set_not_lit_block, &[]);

        self.builder.switch_to_block(set_not_lit_block);
        self.builder.seal_block(set_not_lit_block);
        let new_output_power = self.set_data_imm(self.output_power_data[self.node_idx], 0);
        let new_output_power_i32 = self.builder.ins().uextend(types::I32, new_output_power);
        self.call_set_node(backend, self.node_idx, new_output_power_i32, false);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_redstone_wire_update(&mut self, backend: Value) {
        let return_block = self.builder.create_block();
        let (input_power, _) = self.translate_node_input_power(&self.node.inputs);
        let old_power = self.get_data(self.output_power_data[self.node_idx]);

        let set_power_block = self.builder.create_block();
        self.builder.ins().br_icmp(
            IntCC::NotEqual,
            input_power,
            old_power,
            set_power_block,
            &[],
        );
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(set_power_block);
        self.builder.seal_block(set_power_block);
        self.set_data(self.output_power_data[self.node_idx], input_power);
        self.call_set_node(backend, self.node_idx, input_power, false);
        self.builder.ins().jump(return_block, &[]);

        self.builder.switch_to_block(return_block);
        self.builder.seal_block(return_block);
    }

    fn translate_lever_use(&mut self, entry_block: cranelift::prelude::Block, _lever: Lever) {
        let backend = self.builder.block_params(entry_block)[0];
        let powered_i32 = self.get_data(self.output_power_data[self.node_idx]);
        let powered = self
            .builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, powered_i32, 0);
        let no_power = self.builder.ins().iconst(types::I32, 0);
        let full_power = self.builder.ins().iconst(types::I32, 15);
        let new_power = self.builder.ins().select(powered, no_power, full_power);
        self.set_data(self.output_power_data[self.node_idx], new_power);
        self.call_set_node(backend, self.node_idx, new_power, true);
        self.builder.ins().return_(&[]);
    }
}

pub struct CraneliftBackend {
    // Compilation
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
    module: JITModule,
    // Execution
    nodes: Vec<CompileNode>,
    output_power_data: Vec<DataId>,
    comparator_output_data: Vec<DataId>,
    tick_fns: Vec<FuncId>,
    use_fns: HashMap<BlockPos, FuncId>,
    pos_map: HashMap<BlockPos, usize>,
    to_be_ticked: Vec<CLTickEntry>,
    change_queue: Vec<(BlockPos, Block)>,
}

impl Default for CraneliftBackend {
    fn default() -> Self {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names());
        builder.symbol(
            "cranelift_jit_schedule_tick",
            cranelift_jit_schedule_tick as *const u8,
        );
        builder.symbol(
            "cranelift_jit_pending_tick_at",
            cranelift_jit_pending_tick_at as *const u8,
        );
        builder.symbol(
            "cranelift_jit_set_node",
            cranelift_jit_set_node as *const u8,
        );
        builder.symbol(
            "cranelift_jit_set_locked",
            cranelift_jit_set_locked as *const u8,
        );
        builder.symbol(
            "cranelift_jit_debug_val",
            cranelift_jit_debug_val as *const u8,
        );
        let module = JITModule::new(builder);
        Self {
            builder_context: FunctionBuilderContext::new(),
            ctx: module.make_context(),
            module,
            nodes: Default::default(),
            output_power_data: Default::default(),
            comparator_output_data: Default::default(),
            tick_fns: Default::default(),
            use_fns: Default::default(),
            pos_map: Default::default(),
            to_be_ticked: Default::default(),
            change_queue: Default::default(),
        }
    }
}

impl CraneliftBackend {
    /// Safety:
    /// Function must have signature fn(&mut CraneliftBackend) -> ()
    unsafe fn run_code(&mut self, func_id: FuncId) {
        let code_ptr = self.module.get_finalized_function(func_id);
        let code_fn = std::mem::transmute::<_, extern "C" fn(&mut CraneliftBackend)>(code_ptr);
        code_fn(self)
    }

    unsafe fn get_data(&mut self, data_id: DataId) -> u8 {
        let (data_ptr, _) = self.module.get_finalized_data(data_id);
        *data_ptr
    }
}

impl JITBackend for CraneliftBackend {
    fn compile(&mut self, nodes: Vec<CompileNode>, ticks: Vec<TickEntry>) {
        let mut data_ctx = DataContext::new();

        let mut repeater_lock_data = Vec::new();
        for (idx, node) in nodes.iter().enumerate() {
            let output_power_name = format!("n{}_output_power", idx);
            let comparator_output_name = format!("n{}_comparator_output", idx);
            let repeater_lock_name = format!("n{}_repeater_lock", idx);

            let power = match node.state {
                Block::RedstoneWire { wire } => wire.power,
                Block::RedstoneComparator { comparator } => {
                    comparator.powered.then(|| 15).unwrap_or(0)
                }
                Block::RedstoneTorch { lit } => lit.then(|| 15).unwrap_or(0),
                Block::RedstoneWallTorch { lit, .. } => lit.then(|| 15).unwrap_or(0),
                Block::RedstoneRepeater { repeater } => repeater.powered.then(|| 15).unwrap_or(0),
                Block::Lever { lever } => lever.powered.then(|| 15).unwrap_or(0),
                Block::StoneButton { button } => button.powered.then(|| 15).unwrap_or(0),
                Block::RedstoneBlock {} => 15,
                Block::RedstoneLamp { lit } => lit.then(|| 15).unwrap_or(0),
                _ => 0,
            };
            data_ctx.define(Box::new([power]));
            let output_power_id = self
                .module
                .declare_data(&output_power_name, Linkage::Local, true, false)
                .unwrap();
            self.output_power_data.push(output_power_id);
            self.module.define_data(output_power_id, &data_ctx).unwrap();
            data_ctx.clear();

            let comparator_power = match nodes[idx].state {
                Block::RedstoneComparator { .. } => nodes[idx].comparator_output,
                s if s.has_comparator_override() => nodes[idx].comparator_output,
                _ => 0,
            };
            data_ctx.define(Box::new([comparator_power]));
            let comparator_output_id = self
                .module
                .declare_data(&comparator_output_name, Linkage::Local, true, false)
                .unwrap();
            self.comparator_output_data.push(comparator_output_id);
            self.module
                .define_data(comparator_output_id, &data_ctx)
                .unwrap();
            data_ctx.clear();

            let repeater_lock = match nodes[idx].state {
                Block::RedstoneRepeater { repeater } => repeater.locked as u8,
                _ => 0,
            };
            data_ctx.define(Box::new([repeater_lock]));
            let repeater_lock_id = self
                .module
                .declare_data(&repeater_lock_name, Linkage::Local, true, false)
                .unwrap();
            repeater_lock_data.push(repeater_lock_id);
            self.module
                .define_data(repeater_lock_id, &data_ctx)
                .unwrap();
            data_ctx.clear();
        }

        for (idx, node) in nodes.iter().enumerate() {
            let ptr_type = self.module.target_config().pointer_type();

            self.ctx.func.signature.params.push(AbiParam::new(ptr_type));
            let mut update_builder =
                FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);
            let update_entry_block = update_builder.create_block();
            update_builder.append_block_params_for_function_params(update_entry_block);
            update_builder.switch_to_block(update_entry_block);
            update_builder.seal_block(update_entry_block);

            let mut update_translator = FunctionTranslator {
                builder: update_builder,
                module: &mut self.module,
                comparator_output_data: &self.comparator_output_data,
                output_power_data: &self.output_power_data,
                repeater_lock_data: &repeater_lock_data,
                node,
                node_idx: idx,
                nodes: &nodes,
            };
            update_translator.translate_update(update_entry_block);
            debug!(
                "n{}_update generated {}",
                idx, &update_translator.builder.func
            );

            update_translator.builder.finalize();
            let update_id = self
                .module
                .declare_function(
                    &format!("n{}_update", idx),
                    Linkage::Export,
                    &self.ctx.func.signature,
                )
                .unwrap();
            self.module
                .define_function(
                    update_id,
                    &mut self.ctx,
                    &mut codegen::binemit::NullTrapSink {},
                    &mut codegen::binemit::NullStackMapSink {},
                )
                .unwrap();
            self.module.clear_context(&mut self.ctx);

            self.ctx.func.signature.params.push(AbiParam::new(ptr_type));
            let mut tick_builder =
                FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);
            let tick_entry_block = tick_builder.create_block();
            tick_builder.append_block_params_for_function_params(tick_entry_block);
            tick_builder.switch_to_block(tick_entry_block);
            tick_builder.seal_block(tick_entry_block);

            let mut tick_translator = FunctionTranslator {
                builder: tick_builder,
                module: &mut self.module,
                comparator_output_data: &self.comparator_output_data,
                output_power_data: &self.output_power_data,
                repeater_lock_data: &repeater_lock_data,
                node,
                node_idx: idx,
                nodes: &nodes,
            };
            tick_translator.translate_tick(tick_entry_block);
            // debug!("n{}_tick generated {}", idx, &tick_translator.builder.func);

            tick_translator.builder.finalize();
            let tick_id = self
                .module
                .declare_function(
                    &format!("n{}_tick", idx),
                    Linkage::Export,
                    &self.ctx.func.signature,
                )
                .unwrap();
            self.tick_fns.push(tick_id);
            self.module
                .define_function(
                    tick_id,
                    &mut self.ctx,
                    &mut codegen::binemit::NullTrapSink {},
                    &mut codegen::binemit::NullStackMapSink {},
                )
                .unwrap();
            self.module.clear_context(&mut self.ctx);

            if matches!(node.state, Block::Lever { .. }) {
                self.ctx.func.signature.params.push(AbiParam::new(ptr_type));
                let mut use_builder =
                    FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_context);
                let use_entry_block = use_builder.create_block();
                use_builder.append_block_params_for_function_params(use_entry_block);
                use_builder.switch_to_block(use_entry_block);
                use_builder.seal_block(use_entry_block);

                let mut use_translator = FunctionTranslator {
                    builder: use_builder,
                    module: &mut self.module,
                    comparator_output_data: &self.comparator_output_data,
                    output_power_data: &self.output_power_data,
                    repeater_lock_data: &repeater_lock_data,
                    node,
                    node_idx: idx,
                    nodes: &nodes,
                };
                match node.state {
                    Block::Lever { lever } => {
                        use_translator.translate_lever_use(use_entry_block, lever)
                    }
                    // Block::StoneButton { button } => use_translator.translate_button_use(use_entry_block),
                    _ => unreachable!(),
                }

                // debug!("n{}_use generated {}", idx, &use_translator.builder.func);

                use_translator.builder.finalize();
                let use_id = self
                    .module
                    .declare_function(
                        &format!("n{}_use", idx),
                        Linkage::Export,
                        &self.ctx.func.signature,
                    )
                    .unwrap();
                self.use_fns.insert(node.pos, use_id);
                self.module
                    .define_function(
                        use_id,
                        &mut self.ctx,
                        &mut codegen::binemit::NullTrapSink {},
                        &mut codegen::binemit::NullStackMapSink {},
                    )
                    .unwrap();
                self.module.clear_context(&mut self.ctx);
            }
        }

        self.module.finalize_definitions();

        for (i, node) in nodes.iter().enumerate() {
            self.pos_map.insert(node.pos, i);
        }

        for entry in ticks {
            self.to_be_ticked.push(CLTickEntry {
                ticks_left: entry.ticks_left,
                priority: entry.tick_priority,
                node_id: self.pos_map[&entry.pos],
            })
        }

        self.nodes = nodes;
    }

    fn tick(&mut self) {
        self.to_be_ticked
            .sort_by_key(|e| (e.ticks_left, e.priority));
        for pending in &mut self.to_be_ticked {
            pending.ticks_left = pending.ticks_left.saturating_sub(1);
        }
        while self.to_be_ticked.first().map(|e| e.ticks_left).unwrap_or(1) == 0 {
            let entry = self.to_be_ticked.remove(0);
            // debug!("Calling tick function at {}", entry.node_id);
            unsafe {
                self.run_code(self.tick_fns[entry.node_id]);
            }
        }
    }

    fn on_use_block(&mut self, pos: BlockPos) {
        unsafe {
            self.run_code(self.use_fns[&pos]);
        }
    }

    fn reset(&mut self) -> JITResetData {
        self.tick_fns.clear();
        self.use_fns.clear();

        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names());
        builder.symbol(
            "cranelift_jit_schedule_tick",
            cranelift_jit_schedule_tick as *const u8,
        );
        builder.symbol(
            "cranelift_jit_pending_tick_at",
            cranelift_jit_pending_tick_at as *const u8,
        );
        builder.symbol(
            "cranelift_jit_set_node",
            cranelift_jit_set_node as *const u8,
        );
        builder.symbol(
            "cranelift_jit_set_locked",
            cranelift_jit_set_locked as *const u8,
        );
        builder.symbol(
            "cranelift_jit_debug_val",
            cranelift_jit_debug_val as *const u8,
        );
        let module = JITModule::new(builder);
        let old_module = std::mem::replace(&mut self.module, module);
        // Safe because function pointers have been cleared and there shouldn't be
        // code running on another thread.
        unsafe {
            old_module.free_memory();
        }

        let mut ticks = Vec::new();
        for entry in self.to_be_ticked.drain(..) {
            ticks.push(TickEntry {
                ticks_left: entry.ticks_left,
                tick_priority: entry.priority,
                pos: self.nodes[entry.node_id].pos,
            })
        }

        JITResetData {
            tick_entries: ticks,
            // TODO: collect these
            block_entities: Vec::new(),
        }
    }

    fn block_changes(&mut self) -> &mut Vec<(BlockPos, blocks::Block)> {
        &mut self.change_queue
    }
}

#[repr(C)]
#[derive(Debug)]
enum CLTickPriority {
    Normal,
    High,
    Higher,
    Highest,
}

#[no_mangle]
extern "C" fn cranelift_jit_schedule_tick(
    backend: &mut CraneliftBackend,
    node_id: usize,
    delay: u32,
    priority: CLTickPriority,
) {
    // debug!(
    //     "cranelift_jit_schedule_tick({}, {}, {:?})",
    //     node_id, delay, priority
    // );
    backend.to_be_ticked.push(CLTickEntry {
        ticks_left: delay,
        priority: match priority {
            CLTickPriority::Normal => TickPriority::Normal,
            CLTickPriority::High => TickPriority::High,
            CLTickPriority::Higher => TickPriority::Higher,
            CLTickPriority::Highest => TickPriority::Highest,
            // _ => panic!("Cranelift JIT scheduled tick with priority of {}", priority as u32),
        },
        node_id,
    })
}

#[no_mangle]
extern "C" fn cranelift_jit_pending_tick_at(
    backend: &mut CraneliftBackend,
    node_id: usize,
) -> bool {
    // debug!("cranelift_jit_pending_tick_at({})", node_id);
    backend.to_be_ticked.iter().any(|e| e.node_id == node_id)
}

#[no_mangle]
extern "C" fn cranelift_jit_set_node(backend: &mut CraneliftBackend, node_id: usize, power: u32) {
    // debug!("cranelift_jit_set_node({}, {})", node_id, power);
    let powered = power > 0;
    match &mut backend.nodes[node_id].state {
        Block::RedstoneComparator { comparator } => comparator.powered = powered,
        Block::RedstoneTorch { lit } => *lit = powered,
        Block::RedstoneWallTorch { lit, .. } => *lit = powered,
        Block::RedstoneRepeater { repeater } => repeater.powered = powered,
        Block::RedstoneWire { wire } => wire.power = power as u8,
        Block::Lever { lever } => lever.powered = powered,
        Block::StoneButton { button } => button.powered = powered,
        Block::RedstoneLamp { lit } => *lit = powered,
        _ => {}
    }
    backend
        .change_queue
        .push((backend.nodes[node_id].pos, backend.nodes[node_id].state))
}

#[no_mangle]
extern "C" fn cranelift_jit_set_locked(
    backend: &mut CraneliftBackend,
    node_id: usize,
    locked: bool,
) {
    // debug!("cranelift_jit_set_locked({}, {})", node_id, locked);
    match &mut backend.nodes[node_id].state {
        Block::RedstoneRepeater { repeater } => repeater.locked = locked,
        _ => panic!("cranelift jit tried to lock a node which wasn't a repeater"),
    }
    backend
        .change_queue
        .push((backend.nodes[node_id].pos, backend.nodes[node_id].state))
}

#[no_mangle]
extern "C" fn cranelift_jit_debug_val(val: i32) {
    debug!("cranelift_jit_debug_val({})", val);
}
#[test]
fn test_cranelift_jit_comparator() {
    let mut jit: CraneliftBackend = Default::default();
    let nodes = vec![CompileNode::new(
        BlockPos::new(0, 0, 0),
        Block::RedstoneComparator {
            comparator: Default::default(),
        },
        false,
    )];
    jit.compile(nodes, vec![]);
}
