use crate::block_entities::ContainerType;
pub use crate::generated::Item;
use mchprs_utils::map;

#[derive(Clone, Debug)]
pub struct ItemStack {
    pub item_type: Item,
    pub count: u8,
    pub nbt: Option<nbt::Blob>,
}

impl ItemStack {
    /// Create container item with specified signal strength
    pub fn container_with_ss(container_ty: ContainerType, ss: u8) -> ItemStack {
        let item = match container_ty {
            ContainerType::Barrel => Item::Barrel {},
            ContainerType::Hopper => Item::Hopper {},
            ContainerType::Furnace => Item::Furnace {},
        };

        let items_needed = container_ty.items_needed_for_signal_strength(ss);

        let list = nbt::Value::List({
            let mut items = Vec::new();
            for (slot, items_added) in (0..items_needed).step_by(64).enumerate() {
                let count = (items_needed - items_added).min(64);
                items.push(nbt::Value::Compound(map! {
                    "Count" => nbt::Value::Byte(count as i8),
                    "id" => nbt::Value::String("minecraft:redstone".to_owned()),
                    "Slot" => nbt::Value::Byte(slot as i8)
                }));
            }
            items
        });

        let nbt = nbt::Blob::with_content(map! {
            "BlockEntityTag" => nbt::Value::Compound(map! {
                "Items" => list,
                "Id" => nbt::Value::String(container_ty.to_string())
            })
        });

        ItemStack {
            item_type: item,
            count: 1,
            nbt: Some(nbt),
        }
    }
}
