use mchprs_blocks::blocks::Block;
use mchprs_redstone::noteblock;
use mchprs_world::World;

use crate::{
    backend::direct::{DirectBackend, Event},
    block_powered_mut,
};

impl DirectBackend {
    pub(crate) fn flush_events<W: World>(&mut self, world: &mut W) {
        for event in self.execution_context.drain_events() {
            match event {
                Event::NoteBlockPlay { noteblock_id } => {
                    let (pos, instrument, note) = self.noteblock_info[noteblock_id as usize];
                    noteblock::play_note(world, pos, instrument, note);
                }
            }
        }
    }

    pub(crate) fn flush_block_changes<W: World>(&mut self, world: &mut W) {
        for i in self.execution_context.drain_changes() {
            let node = &mut self.nodes[i];
            let Some((pos, block)) = &mut self.blocks[i.index()] else {
                continue;
            };
            if let Some(powered) = block_powered_mut(block) {
                *powered = node.powered
            }
            if let Block::RedstoneWire { wire, .. } = block {
                wire.power = node.output_power
            };
            if let Block::RedstoneRepeater { repeater } = block {
                repeater.locked = node.locked;
            }
            world.set_block(*pos, *block);
            node.changed = false;
        }
    }

    pub(super) fn flush_scheduled_ticks<W: World>(&mut self, world: &mut W) {
        for (delay, node_id, priority) in self.execution_context.drain_scheduled_ticks() {
            let Some((pos, _)) = &self.blocks[node_id.index()] else {
                continue;
            };
            world.schedule_tick(*pos, delay as u32, priority);
        }
    }
}
