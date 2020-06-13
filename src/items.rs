use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::Plot;

#[derive(PartialEq, Copy, Clone)]
pub enum ActionResult {
    Success,
    Pass,
}

impl ActionResult {
    fn is_success(self) -> bool {
        self == ActionResult::Success
    }
}

// Debug is temporary
#[derive(Debug)]
pub struct UseOnBlockContext {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player_crouching: bool,
    pub player_direction: BlockDirection,
    pub player_idx: usize,
}

#[derive(Clone)]
pub struct ItemStack {
    pub item_type: Item,
    pub count: u8,
    pub damage: u16,
    pub nbt: Option<nbt::Blob>,
}

impl ItemStack {
    pub fn use_on_block(&self, plot: &mut Plot, context: UseOnBlockContext) {
        let pos = context.block_pos;
        let block = plot.get_block(pos);

        match self.item_type {
            Item::WEWand => {
                if let Some(first_pos) = plot.players[context.player_idx].second_position {
                    if pos != first_pos {
                        plot.players[context.player_idx]
                            .worldedit_set_second_position(pos.x, pos.y, pos.z);
                    }
                } else {
                    plot.players[context.player_idx]
                        .worldedit_set_second_position(pos.x, pos.y, pos.z);
                }
            }
            _ => {}
        }

        if !context.player_crouching && block.on_use(plot, context.block_pos).is_success() {
            return;
        }

        let block_pos = context.block_pos.offset(context.block_face);
        if let Item::BlockItem(item_id) = self.item_type {
            if plot.get_block(block_pos).can_place_block_in() {
                let block = Block::get_state_for_placement(plot, block_pos, item_id, &context);
                block.place_in_plot(plot, block_pos);
            }
        } else {
            // This is to make sure the client doesn't place a block
            // that the server can't handle.

            let block = plot.get_block(context.block_pos);
            plot.send_block_change(context.block_pos, block.get_id());

            let offset_block = plot.get_block(block_pos);
            plot.send_block_change(block_pos, offset_block.get_id());
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// BlockItem represents an item that can be placed down. The u32 is the id of the item,
    /// it is NOT a block state id.
    BlockItem(u32),
    WEWand,
    Unknown(u32),
}

impl Item {
    pub fn from_id(id: u32) -> Item {
        match id {
            64 => Item::BlockItem(id),
            68 => Item::BlockItem(id),
            82..=97 => Item::BlockItem(id),
            164 => Item::BlockItem(id),
            173 => Item::BlockItem(id),
            234 => Item::BlockItem(id),
            272 => Item::BlockItem(id),
            281..=296 => Item::BlockItem(id),
            413..=428 => Item::BlockItem(id),
            513..=514 => Item::BlockItem(id),
            536 => Item::WEWand,
            600 => Item::BlockItem(id),
            _ => Item::Unknown(id),
        }
    }

    pub fn get_id(&self) -> u32 {
        match self {
            Item::WEWand => 536,
            Item::BlockItem(id) => *id,
            Item::Unknown(id) => *id,
        }
    }
}
