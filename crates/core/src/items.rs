use mchprs_blocks::items::ItemStack;
use mchprs_blocks::{BlockDirection, BlockFace, BlockPos};

use crate::blocks::Block;
use crate::config::CONFIG;
use crate::network::packets::clientbound::{COpenSignEditor, ClientBoundPacket};
use crate::plot::Plot;
use crate::world::World;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum ActionResult {
    Success,
    Pass,
}

impl ActionResult {
    fn is_success(self) -> bool {
        self == ActionResult::Success
    }
}

pub struct UseOnBlockContext {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player_crouching: bool,
    pub player_direction: BlockDirection,
    pub player_yaw: f32,
    /// The index of the player in the plot's player array
    pub player_idx: usize,
}

/// returns true if cancelled
pub fn use_item_on_block(item: &ItemStack, plot: &mut Plot, context: UseOnBlockContext) -> bool {
    let use_pos = context.block_pos;
    let use_block = plot.world.get_block(use_pos);
    let block_pos = context.block_pos.offset(context.block_face);
    let mut top_pos = plot.players[context.player_idx].pos.block_pos();
    top_pos.y += 1;
    if (block_pos == plot.players[context.player_idx].pos.block_pos() || block_pos == top_pos)
        && !CONFIG.block_in_hitbox
    {
        return false;
    }
    let can_place =
        item.item_type.is_block() && plot.world.get_block(block_pos).can_place_block_in();

    if !context.player_crouching
        && use_block
            .on_use(
                &mut plot.world,
                &mut plot.players[context.player_idx],
                context.block_pos,
                Some(item.item_type),
            )
            .is_success()
    {
        return false;
    }

    if can_place && (0..256).contains(&block_pos.y) {
        let block =
            Block::get_state_for_placement(&plot.world, block_pos, item.item_type, &context);

        match block {
            Block::Sign { .. } | Block::WallSign { .. } => {
                let open_sign_editor = COpenSignEditor {
                    pos_x: block_pos.x,
                    pos_y: block_pos.y,
                    pos_z: block_pos.z,
                }
                .encode();
                plot.players[context.player_idx]
                    .client
                    .send_packet(&open_sign_editor);
            }
            _ => {}
        }

        block.place_in_world(&mut plot.world, block_pos, &item.nbt);
        false
    } else {
        true
    }
}
