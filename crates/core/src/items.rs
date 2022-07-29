use crate::blocks::Block;
use crate::config::CONFIG;
use crate::player::Player;
use crate::plot::PlotWorld;
use crate::world::World;
use mchprs_blocks::items::ItemStack;
use mchprs_blocks::{BlockFace, BlockPos};
use mchprs_network::packets::clientbound::{COpenSignEditor, ClientBoundPacket};

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

pub struct UseOnBlockContext<'a> {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player: &'a mut Player,
}

/// returns true if cancelled
pub fn use_item_on_block(
    item: &ItemStack,
    world: &mut PlotWorld,
    ctx: UseOnBlockContext<'_>,
) -> bool {
    let use_pos = ctx.block_pos;
    let use_block = world.get_block(use_pos);
    let block_pos = ctx.block_pos.offset(ctx.block_face);
    let mut top_pos = ctx.player.pos.block_pos();
    top_pos.y += 1;
    if (block_pos == ctx.player.pos.block_pos() || block_pos == top_pos) && !CONFIG.block_in_hitbox
    {
        return false;
    }
    let can_place = item.item_type.is_block() && world.get_block(block_pos).can_place_block_in();

    if !ctx.player.crouching
        && use_block
            .on_use(world, ctx.player, ctx.block_pos, Some(item.item_type))
            .is_success()
    {
        return false;
    }

    if can_place && (0..256).contains(&block_pos.y) {
        let block = Block::get_state_for_placement(world, block_pos, item.item_type, &ctx);

        match block {
            Block::Sign { .. } | Block::WallSign { .. } => {
                let open_sign_editor = COpenSignEditor {
                    pos_x: block_pos.x,
                    pos_y: block_pos.y,
                    pos_z: block_pos.z,
                }
                .encode();
                ctx.player.client.send_packet(&open_sign_editor);
            }
            _ => {}
        }

        block.place_in_world(world, block_pos, &item.nbt);
        false
    } else {
        true
    }
}
