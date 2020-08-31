use crate::blocks::{Block, BlockDirection, BlockFace, BlockPos};
use crate::plot::Plot;
use crate::world::World;

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

pub struct UseOnBlockContext {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player_crouching: bool,
    pub player_direction: BlockDirection,
    /// The index of the player in the plot's player array
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

        if !context.player_crouching
            && block
                .on_use(plot, context.block_pos, Some(self.item_type))
                .is_success()
        {
            return;
        }

        let block_pos = context.block_pos.offset(context.block_face);
        if let Item::BlockItem(item_id) = self.item_type {
            if plot.get_block(block_pos).can_place_block_in() {
                let block = Block::get_state_for_placement(plot, block_pos, item_id, &context);
                block.place_in_world(plot, block_pos, &self.nbt);
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

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum Item {
    /// Represents an item that can be placed down. It contains the id of the item,
    /// NOT the block state id.
    BlockItem(u32),
    /// This is hardcoded to be the wooden axe.
    WEWand,
    Snowball,
    TotemOfUndying,
    Unknown(u32),
}

impl Item {
    pub fn from_id(id: u32) -> Item {
        match id {
            77 => Item::BlockItem(id),
            81 => Item::BlockItem(id),
            93 => Item::BlockItem(id),
            95..=110 => Item::BlockItem(id),
            185 => Item::BlockItem(id),
            189 => Item::BlockItem(id),
            201 => Item::BlockItem(id),
            274 => Item::BlockItem(id),
            304 => Item::BlockItem(id),
            321 => Item::BlockItem(id),
            323 => Item::BlockItem(id),
            331..=346 => Item::BlockItem(id),
            464..=479 => Item::BlockItem(id),
            566..=567 => Item::BlockItem(id),
            586 => Item::WEWand,
            665 => Item::BlockItem(id),
            666 => Item::Snowball,
            904 => Item::TotemOfUndying,
            936 => Item::BlockItem(id),
            961 => Item::BlockItem(id),
            _ => Item::Unknown(id),
        }
    }

    pub fn get_id(self) -> u32 {
        match self {
            Item::WEWand => 586,
            Item::Snowball => 666,
            Item::TotemOfUndying => 904,
            Item::BlockItem(id) => id,
            Item::Unknown(id) => id,
        }
    }

    pub fn from_name(name: &str) -> Option<Item> {
        match name {
            "snowball" => Some(Item::Snowball),
            "totem_of_undying" => Some(Item::TotemOfUndying),
            _ => None,
        }
    }

    pub fn max_stack_size(self) -> u32 {
        match self {
            Item::Snowball => 16,
            Item::TotemOfUndying => 1,
            _ => 64,
        }
    }
}
