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

pub struct UseOnBlockContext {
    pub block_pos: BlockPos,
    pub block_face: BlockFace,
    pub player_crouching: bool,
    pub player_direction: BlockDirection,
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
        let block = plot.get_block(&context.block_pos);

        if !context.player_crouching && block.on_use(plot, &context.block_pos).is_success() {
            return;
        }

        let block_pos = context.block_pos.offset(context.block_face);
        if let Item::BlockItem(item_id) = self.item_type {
            if plot.get_block(&block_pos) == Block::Air {
                let block = Block::get_block_for_placement(item_id, &context);
                block.place_in_plot(plot, &block_pos);
            }
        } else {
            // This is to make sure the client doesn't place a block
            // that the server can't handle.

            let block = plot.get_block(&context.block_pos);
            plot.send_block_change(&context.block_pos, block.get_id());

            let offset_block = plot.get_block(&block_pos);
            plot.send_block_change(&block_pos, offset_block.get_id());
        }
    }
}

#[derive(Clone, Debug)]
pub enum Item {
    /// BlockItem represents an item that can be placed down. The u32 is the id of the item, 
    /// it is NOT a block state id.
    BlockItem(u32),
    Unknown(u32),
}

impl Item {
    pub fn from_id(id: u32) -> Item {
        dbg!(id);
        match id {
            64 => Item::BlockItem(id), 
            68 => Item::BlockItem(id),
            82..=97 => Item::BlockItem(id),
            173 => Item::BlockItem(id),
            413..=428 => Item::BlockItem(id),
            513 => Item::BlockItem(id),
            
            _ => Item::Unknown(id),
        }
    }

    pub fn get_id(&self) -> u32 {
        match self {
            Item::BlockItem(id) => *id,
            Item::Unknown(id) => *id,
        }
    }
}
