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
        get_id: 817,
        from_id(_id): 817 => {},
    },
    Snowball {
        props: {},
        get_id: 909,
        from_id(_id): 909 => {},
        max_stack: 16,
    },
    TotemOfUndying {
        props: {},
        get_id: 1156,
        from_id(_id): 1156 => {},
        max_stack: 1,
    },
    MilkBucket {
        props: {},
        get_id: 911,
        from_id(_id): 911 => {},
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
        get_id: 656,
        from_id(_id): 656 => {},
        block: true,
    },
    Glass {
        props: {},
        get_id: 187,
        from_id(_id): 187 => {},
        block: true,
    },
    Sandstone {
        props: {},
        get_id: 190,
        from_id(_id): 190 => {},
        block: true,
    },
    SeaPickle {
        props: {},
        get_id: 200,
        from_id(_id): 200 => {},
        block: true,
    },
    Wool {
        props: {
            color: BlockColorVariant
        },
        get_id: 201 + color.get_id(),
        from_id_offset: 201,
        from_id(id): 201..=216 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Furnace {
        props: {},
        get_id: 301,
        from_id(_id): 301 => {},
        block: true,
    },
    Lever {
        props: {},
        get_id: 671,
        from_id(_id): 671 => {},
        block: true,
    },
    StonePressurePlate {
        props: {},
        get_id: 694,
        from_id(_id): 694 => {},
        block: true,
    },
    RedstoneTorch {
        props: {},
        get_id: 657,
        from_id(_id): 657 => {},
        block: true,
    },
    StoneButton {
        props: {},
        get_id: 681,
        from_id(_id): 681 => {},
        block: true,
    },
    RedstoneLamp {
        props: {},
        get_id: 679,
        from_id(_id): 679 => {},
        block: true,
    },
    RedstoneBlock {
        props: {},
        get_id: 658,
        from_id(_id): 658 => {},
        block: true,
    },
    Hopper {
        props: {},
        get_id: 666,
        from_id(_id): 666 => {},
        block: true,
    },
    TripwireHook {
        props: {},
        get_id: 676,
        from_id(_id): 676 => {},
        block: true,
    },
    Terracotta {
        props: {},
        get_id: 461,
        from_id(_id): 461 => {},
        block: true,
    },
    ColoredTerracotta {
        props: {
            color: BlockColorVariant
        },
        get_id: 426 + color.get_id(),
        from_id_offset: 426,
        from_id(id): 426..=441 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Concrete {
        props: {
            color: BlockColorVariant
        },
        get_id: 554 + color.get_id(),
        from_id_offset: 554,
        from_id(id): 554..=569 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    StainedGlass {
        props: {
            color: BlockColorVariant
        },
        get_id: 470 + color.get_id(),
        from_id_offset: 470,
        from_id(id): 470..=485 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Repeater {
        props: {},
        get_id: 659,
        from_id(_id): 659 => {},
        block: true,
    },
    Comparator {
        props: {},
        get_id: 660,
        from_id(_id): 660 => {},
        block: true,
    },
    Sign {
        props: {
            sign_type: u32
        },
        get_id: 883 + sign_type,
        from_id_offset: 883,
        from_id(id): 883..=893 => {
            sign_type: id
        },
        block: true,
    },
    Barrel {
        props: {},
        get_id: 1193,
        from_id(_id): 1193 => {},
        block: true,
    },
    Target {
        props: {},
        get_id: 670,
        from_id(_id): 670 => {},
        block: true,
    },
    SmoothStoneSlab {
        props: {},
        get_id: 264,
        from_id(_id): 264 => {},
        block: true,
    },
    QuartzSlab {
        props: {},
        get_id: 273,
        from_id(_id): 273 => {},
        block: true,
    },
    IronTrapdoor {
        props: {},
        get_id: 729,
        from_id(_id): 729 => {},
        block: true,
    },
    NoteBlock {
        props: {},
        get_id: 680,
        from_id(_id): 680 => {},
        block: true,
    },
    Clay {
        props: {},
        get_id: 308,
        from_id(_id): 308 => {},
        block: true,
    },
    GoldBlock {
        props: {},
        get_id: 89,
        from_id(_id): 89 => {},
        block: true,
    },
    PackedIce {
        props: {},
        get_id: 462,
        from_id(_id): 462 => {},
        block: true,
    },
    BoneBlock {
        props: {},
        get_id: 519,
        from_id(_id): 519 => {},
        block: true,
    },
    IronBlock {
        props: {},
        get_id: 87,
        from_id(_id): 87 => {},
        block: true,
    },
    SoulSand {
        props: {},
        get_id: 325,
        from_id(_id): 325 => {},
        block: true,
    },
    Pumpkin {
        props: {},
        get_id: 321,
        from_id(_id): 321 => {},
        block: true,
    },
    EmeraldBlock {
        props: {},
        get_id: 381,
        from_id(_id): 381 => {},
        block: true,
    },
    HayBlock {
        props: {},
        get_id: 444,
        from_id(_id): 444 => {},
        block: true,
    },
    Sand {
        props: {},
        get_id: 57,
        from_id(_id): 57 => {},
        block: true,
    },
    StoneBricks {
        props: {},
        get_id: 339,
        from_id(_id): 339 => {},
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
