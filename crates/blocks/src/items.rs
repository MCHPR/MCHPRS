use crate::block_entities::ContainerType;
pub use crate::generated::Item;

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub item_type: Item,
    pub count: u8,
    pub nbt: Option<nbt::Blob>,
    pub container_slots: Vec<Option<ItemStack>>,
}

impl ItemStack {
    /// Create container item with specified signal strength
    pub fn container_with_ss(container_ty: ContainerType, ss: u8) -> ItemStack {
        let item = match container_ty {
            ContainerType::Barrel => Item::Barrel {},
            ContainerType::Hopper => Item::Hopper {},
            ContainerType::Furnace => Item::Furnace {},
        };
        let slots = container_ty.num_slots() as u32;

        let items_needed = match ss {
            0 => 0,
            15 => slots * 64,
            _ => ((32 * slots * ss as u32) as f32 / 7.0 - 1.0).ceil() as u32,
        } as usize;

        let mut container_slots = Vec::new();
        if items_needed > 0 {
            for (_slot, items_added) in (0..items_needed).step_by(64).enumerate() {
                let count = (items_needed - items_added).min(64);
                container_slots.push(Some(ItemStack {
                    item_type: Item::Redstone,
                    count: count as u8,
                    nbt: None,
                    container_slots: Vec::new(),
                }));
            }
        }

        ItemStack {
            item_type: item,
            count: 1,
            nbt: None,
            container_slots,
        }
    }
}
