use serde::{Deserialize, Serialize};

use crate::blocks::{Block, BlockColorVariant, BlockDirection, BlockFace, BlockPos, ContainerType};
use crate::network::packets::clientbound::{COpenSignEditor, ClientBoundPacket};
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
    pub player_yaw: f32,
    /// The index of the player in the plot's player array
    pub player_idx: usize,
}

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
                let mut nbt = nbt::Blob::new();
                let list = nbt::Value::List({
                    let mut items = Vec::new();
                    for (slot, items_added) in (0..items_needed).step_by(64).enumerate() {
                        let count = (items_needed - items_added).min(64);
                        items.push(nbt::Value::Compound(map! {
                            "Count".to_owned() => nbt::Value::Byte(count as i8),
                            "id".to_owned() => nbt::Value::String("minecraft:redstone".to_owned()),
                            "Slot".to_owned() => nbt::Value::Byte(slot as i8)
                        }));
                    }
                    items
                });

                let tag = nbt::Value::Compound(map! {
                    "Items".to_owned() => list,
                    "Id".to_owned() => nbt::Value::String(container_ty.to_string())
                });
                nbt.insert("BlockEntityTag", tag).unwrap();
                nbt
            }),
        };

        ItemStack {
            item_type: item,
            count: 1,
            nbt,
        }
    }
}

/// A single item in an inventory
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub id: u32,
    pub slot: i8,
    pub count: i8,
    pub nbt: Option<Vec<u8>>,
}

impl ItemStack {
    /// returns true if cancelled
    pub fn use_on_block(&self, plot: &mut Plot, context: UseOnBlockContext) -> bool {
        let use_pos = context.block_pos;
        let use_block = plot.world.get_block(use_pos);
        let block_pos = context.block_pos.offset(context.block_face);

        let can_place =
            self.item_type.is_block() && plot.world.get_block(block_pos).can_place_block_in();

        if !context.player_crouching
            && use_block
                .on_use(
                    &mut plot.world,
                    &mut plot.players[context.player_idx],
                    context.block_pos,
                    Some(self.item_type),
                )
                .is_success()
        {
            return false;
        }

        if can_place {
            let block =
                Block::get_state_for_placement(&plot.world, block_pos, self.item_type, &context);

            match block {
                Block::Sign { .. } | Block::WallSign { .. } => {
                    let open_sign_editor = COpenSignEditor {
                        pos_x: block_pos.x,
                        pos_y: block_pos.y,
                        pos_z: block_pos.z,
                    }
                    .encode();
                    plot.players[context.player_idx]
                        .client
                        .send_packet(&open_sign_editor);
                }
                _ => {}
            }

            block.place_in_world(&mut plot.world, block_pos, &self.nbt);
            false
        } else {
            true
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
        #[derive(Clone, Copy, Debug, PartialEq)]
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

            fn is_block(self) -> bool {
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
        from_id(id): 768..=773 => {
            sign_type: id
        },
        block: true,
    },
    Barrel {
        props: {},
        get_id: 1042,
        from_id(_id): 1042 => {},
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
            _ => None,
        }
    }

    pub fn get_name(self) -> &'static str {
        match self {
            Item::Snowball {} => "snowball",
            Item::TotemOfUndying {} => "totem_of_undying",
            _ => "redstone",
        }
    }
}
