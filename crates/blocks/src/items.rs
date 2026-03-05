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
        let slots = container_ty.num_slots() as u32;

        let items_needed = match ss {
            0 => 0,
            15 => slots * 64,
            _ => ((32 * slots * ss as u32) as f32 / 7.0 - 1.0).ceil() as u32,
        } as usize;

        let nbt = match items_needed {
            0 => None,
            _ => Some({
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

                nbt::Blob::with_content(map! {
                    "BlockEntityTag" => nbt::Value::Compound(map! {
                        "Items" => list,
                        "Id" => nbt::Value::String(container_ty.to_string())
                    })
                })
            }),
        };

        ItemStack {
            item_type: item,
            count: 1,
            nbt,
        }
    }
}
