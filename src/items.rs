use crate::blocks::{Block, BlockColorVariant, BlockDirection, BlockFace, BlockPos};
use crate::network::packets::clientbound::{C2EOpenSignEditor, ClientBoundPacket};
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
    pub damage: u16,
    pub nbt: Option<nbt::Blob>,
}

impl ItemStack {
    pub fn use_on_block(&self, plot: &mut Plot, context: UseOnBlockContext) {
        let pos = context.block_pos;
        let block = plot.get_block(pos);

        match self.item_type {
            Item::WEWand {} => {
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
        if self.item_type.is_block() && plot.get_block(block_pos).can_place_block_in() {
            let block = Block::get_state_for_placement(plot, block_pos, self.item_type, &context);

            match block {
                Block::Sign { .. } | Block::WallSign { .. } => {
                    let open_sign_editor = C2EOpenSignEditor {
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

            block.place_in_world(plot, block_pos, &self.nbt);
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
        get_id: 590,
        from_id(id): 590 => {},
    },
    Snowball {
        props: {},
        get_id: 666,
        from_id(id): 666 => {},
        max_stack: 16,
    },
    TotemOfUndying {
        props: {},
        get_id: 904,
        from_id(id): 904 => {},
        max_stack: 1,
    },
    Redstone {
        props: {},
        get_id: 665,
        from_id(id): 665 => {},
        block: true,
    },
    Glass {
        props: {},
        get_id: 77,
        from_id(id): 77 => {},
        block: true,
    },
    Sandstone {
        props: {},
        get_id: 81,
        from_id(id): 81 => {},
        block: true,
    },
    SeaPickle {
        props: {},
        get_id: 93,
        from_id(id): 93 => {},
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
        from_id(id): 185 => {},
        block: true,
    },
    Lever {
        props: {},
        get_id: 189,
        from_id(id): 189 => {},
        block: true,
    },
    RedstoneTorch {
        props: {},
        get_id: 201,
        from_id(id): 201 => {},
        block: true,
    },
    StoneButton {
        props: {},
        get_id: 304,
        from_id(id): 304 => {},
        block: true,
    },
    RedstoneLamp {
        props: {},
        get_id: 274,
        from_id(id): 274 => {},
        block: true,
    },
    RedstoneBlock {
        props: {},
        get_id: 321,
        from_id(id): 321 => {},
        block: true,
    },
    Hopper {
        props: {},
        get_id: 323,
        from_id(id): 323 => {},
        block: true,
    },
    Terracotta {
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
    Repeater {
        props: {},
        get_id: 566,
        from_id(id): 566 => {},
        block: true,
    },
    Comparator {
        props: {},
        get_id: 567,
        from_id(id): 567 => {},
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
        from_id(id): 936 => {},
        block: true,
    },
    Target {
        props: {},
        get_id: 961,
        from_id(id): 961 => {},
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
