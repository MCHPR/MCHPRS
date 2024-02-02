use crate::block_entities::ContainerType;
use crate::BlockColorVariant;
use mchprs_utils::map;

#[derive(Clone)]
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

macro_rules! items {
    (
        $(
            $name:ident {
                props: {
                    $(
                        $prop_name:ident : $prop_type:ident
                    ),*
                },
                get_id: $get_id:expr,
                $( from_id_offset: $get_id_offset:literal, )?
                from_id($id_name:ident): $from_id_pat:pat => {
                    $(
                        $from_id_pkey:ident: $from_id_pval:expr
                    ),*
                },
                $( max_stack: $max_stack:literal, )?
                $( block: $block:literal, )?
            }
        ),*
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Item {
            $(
                $name {
                    $(
                        $prop_name: $prop_type,
                    )*
                }
            ),*
        }

        #[allow(clippy::redundant_field_names)]
        impl Item {
            pub fn get_id(self) -> u32 {
                match self {
                    $(
                        Item::$name {
                            $(
                                $prop_name,
                            )*
                        } => $get_id,
                    )*
                }
            }

            pub fn from_id(mut id: u32) -> Item {
                match id {
                    $(
                        $from_id_pat => {
                            $( id -= $get_id_offset; )?
                            let $id_name = id;
                            Item::$name {
                                $(
                                    $from_id_pkey: $from_id_pval
                                ),*
                            }
                        },
                    )*
                }
            }

            pub fn is_block(self) -> bool {
                match self {
                    $(
                        $( Item::$name { .. } => $block, )?
                    )*
                    _ => false
                }
            }

            pub fn max_stack_size(self) -> u32 {
                match self {
                    $(
                        $( Item::$name { .. } => $max_stack, )?
                    )*
                    _ => 64,
                }
            }
        }
    }
}

items! {
    // Wooden Axe
    WEWand {
        props: {},
        get_id: 702,
        from_id(_id): 702 => {},
    },
    Snowball {
        props: {},
        get_id: 780,
        from_id(_id): 780 => {},
        max_stack: 16,
    },
    TotemOfUndying {
        props: {},
        get_id: 1010,
        from_id(_id): 1010 => {},
        max_stack: 1,
    },
    MilkBucket {
        props: {},
        get_id: 782,
        from_id(_id): 782 => {},
        max_stack: 1,
    },
    Stone {
        props: {},
        get_id: 1,
        from_id(_id): 1 => {},
        block: true,
    },
    Redstone {
        props: {},
        get_id: 585,
        from_id(_id): 585 => {},
        block: true,
    },
    Glass {
        props: {},
        get_id: 143,
        from_id(_id): 143 => {},
        block: true,
    },
    Sandstone {
        props: {},
        get_id: 146,
        from_id(_id): 146 => {},
        block: true,
    },
    SeaPickle {
        props: {},
        get_id: 156,
        from_id(_id): 156 => {},
        block: true,
    },
    Wool {
        props: {
            color: BlockColorVariant
        },
        get_id: 157 + color.get_id(),
        from_id_offset: 157,
        from_id(id): 157..=172 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Furnace {
        props: {},
        get_id: 248,
        from_id(_id): 248 => {},
        block: true,
    },
    Lever {
        props: {},
        get_id: 600,
        from_id(_id): 600 => {},
        block: true,
    },
    StonePressurePlate {
        props: {},
        get_id: 190,
        from_id(_id): 190 => {},
        block: true,
    },
    RedstoneTorch {
        props: {},
        get_id: 586,
        from_id(_id): 586 => {},
        block: true,
    },
    StoneButton {
        props: {},
        get_id: 609,
        from_id(_id): 609 => {},
        block: true,
    },
    RedstoneLamp {
        props: {},
        get_id: 607,
        from_id(_id): 607 => {},
        block: true,
    },
    RedstoneBlock {
        props: {},
        get_id: 587,
        from_id(_id): 587 => {},
        block: true,
    },
    Hopper {
        props: {},
        get_id: 595,
        from_id(_id): 595 => {},
        block: true,
    },
    TripwireHook {
        props: {},
        get_id: 604,
        from_id(_id): 604 => {},
        block: true,
    },
    Terracotta {
        props: {},
        get_id: 389,
        from_id(_id): 389 => {},
        block: true,
    },
    ColoredTerracotta {
        props: {
            color: BlockColorVariant
        },
        get_id: 354 + color.get_id(),
        from_id_offset: 354,
        from_id(id): 354..=371 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Concrete {
        props: {
            color: BlockColorVariant
        },
        get_id: 484 + color.get_id(),
        from_id_offset: 484,
        from_id(id): 484..=499 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    StainedGlass {
        props: {
            color: BlockColorVariant
        },
        get_id: 400 + color.get_id(),
        from_id_offset: 400,
        from_id(id): 400..=415 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Repeater {
        props: {},
        get_id: 588,
        from_id(_id): 588 => {},
        block: true,
    },
    Comparator {
        props: {},
        get_id: 589,
        from_id(_id): 589 => {},
        block: true,
    },
    Sign {
        props: {
            sign_type: u32
        },
        get_id: 768 + sign_type,
        from_id_offset: 768,
        from_id(id): 768..=775 => {
            sign_type: id
        },
        block: true,
    },
    Barrel {
        props: {},
        get_id: 1043,
        from_id(_id): 1043 => {},
        block: true,
    },
    Target {
        props: {},
        get_id: 599,
        from_id(_id): 599 => {},
        block: true,
    },
    SmoothStoneSlab {
        props: {},
        get_id: 213,
        from_id(_id): 213 => {},
        block: true,
    },
    QuartzSlab {
        props: {},
        get_id: 221,
        from_id(_id): 221 => {},
        block: true,
    },
    IronTrapdoor {
        props: {},
        get_id: 640,
        from_id(_id): 640 => {},
        block: true,
    },
    Unknown {
        props: {
            id: u32
        },
        get_id: id,
        from_id(id): _ => { id: id },
    }
}

impl Item {
    pub fn from_name(name: &str) -> Option<Item> {
        match name {
            "snowball" => Some(Item::Snowball {}),
            "totem_of_undying" => Some(Item::TotemOfUndying {}),
            "milk_bucket" => Some(Item::MilkBucket {}),
            // Convert some common types of items to fix signal strength of containers
            "redstone" => Some(Item::Redstone {}),
            "stick" => Some(Item::Redstone {}),
            "wooden_shovel" => Some(Item::TotemOfUndying {}),
            _ => None,
        }
    }

    pub fn get_name(self) -> &'static str {
        match self {
            Item::Snowball {} => "snowball",
            Item::TotemOfUndying {} => "totem_of_undying",
            Item::MilkBucket {} => "milk_bucket",
            _ => "redstone",
        }
    }
}
