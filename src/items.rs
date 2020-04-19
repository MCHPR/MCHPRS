use crate::blocks::Block;

enum Item {
    /// BlockItem represents an item that can be placed down. 
    BlockItem(Block),
    Unknown(u32),
}

impl Item {
    fn from(id: u32) -> Item {
        match id {

            _ => Item::Unknown(id)
        }
    }
}
