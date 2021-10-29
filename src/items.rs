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
                        let count = if items_added > items_needed {
                            items_added - items_needed
                        } else {
                            64
                        };
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
        get_id: 586,
        from_id(_id): 586 => {},
    },
    Snowball {
        props: {},
        get_id: 666,
        from_id(_id): 666 => {},
        max_stack: 16,
    },
    TotemOfUndying {
        props: {},
        get_id: 904,
        from_id(_id): 904 => {},
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
        get_id: 665,
        from_id(_id): 665 => {},
        block: true,
    },
    Glass {
        props: {},
        get_id: 77,
        from_id(_id): 77 => {},
        block: true,
    },
    Sandstone {
        props: {},
        get_id: 81,
        from_id(_id): 81 => {},
        block: true,
    },
    SeaPickle {
        props: {},
        get_id: 93,
        from_id(_id): 93 => {},
        block: true,
    },
    Wool {
        props: {
            color: BlockColorVariant
        },
        get_id: 95 + color.get_id(),
        from_id_offset: 95,
        from_id(id): 95..=110 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Furnace {
        props: {},
        get_id: 185,
        from_id(_id): 185 => {},
        block: true,
    },
    Lever {
        props: {},
        get_id: 189,
        from_id(_id): 189 => {},
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
        get_id: 201,
        from_id(_id): 201 => {},
        block: true,
    },
    StoneButton {
        props: {},
        get_id: 304,
        from_id(_id): 304 => {},
        block: true,
    },
    RedstoneLamp {
        props: {},
        get_id: 274,
        from_id(_id): 274 => {},
        block: true,
    },
    RedstoneBlock {
        props: {},
        get_id: 321,
        from_id(_id): 321 => {},
        block: true,
    },
    Hopper {
        props: {},
        get_id: 323,
        from_id(_id): 323 => {},
        block: true,
    },
    Terracotta {
        props: {},
        get_id: 366,
        from_id(_id): 366 => {},
        block: true,
    },
    ColoredTerracotta {
        props: {
            color: BlockColorVariant
        },
        get_id: 331 + color.get_id(),
        from_id_offset: 331,
        from_id(id): 331..=346 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Concrete {
        props: {
            color: BlockColorVariant
        },
        get_id: 464 + color.get_id(),
        from_id_offset: 464,
        from_id(id): 464..=479 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    StainedGlass {
        props: {
            color: BlockColorVariant
        },
        get_id: 379 + color.get_id(),
        from_id_offset: 379,
        from_id(id): 379..=394 => {
            color: BlockColorVariant::from_id(id)
        },
        block: true,
    },
    Repeater {
        props: {},
        get_id: 566,
        from_id(_id): 566 => {},
        block: true,
    },
    Comparator {
        props: {},
        get_id: 567,
        from_id(_id): 567 => {},
        block: true,
    },
    Sign {
        props: {
            sign_type: u32
        },
        get_id: 652 + sign_type,
        from_id_offset: 652,
        from_id(id): 652..=657 => {
            sign_type: id
        },
        block: true,
    },
    Barrel {
        props: {},
        get_id: 936,
        from_id(_id): 936 => {},
        block: true,
    },
    Target {
        props: {},
        get_id: 961,
        from_id(_id): 961 => {},
        block: true,
    },
    SmoothStoneSlab {
        props: {},
        get_id: 147,
        from_id(_id): 147 => {},
        block: true,
    },
    QuartzSlab {
        props: {},
        get_id: 155,
        from_id(_id): 155 => {},
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
}
