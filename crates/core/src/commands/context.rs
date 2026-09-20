use crate::{
    commands::{
        define::FLAGS_ARGUMENT,
        error::{CommandResult, InternalError, RuntimeError},
        value::{BlockCoordinates, FromValue, PositionCoordinates, Value},
    },
    player::{PacketSender, Player, PlayerPos},
    plot::{Plot, PlotWorld, PLOT_BLOCK_HEIGHT},
    worldedit::{create_clipboard, ray_trace_block, WorldEditClipboard, WorldEditUndo},
};
use mchprs_blocks::BlockPos;
use mchprs_commands::ParsedArgument;
use mchprs_text::TextComponent;

pub struct ExecutionContext<'a> {
    plot: &'a mut Plot,
    player_idx: usize,
    arguments: &'a [ParsedArgument<Value>],
    world_access: WorldAccess,
}

#[derive(PartialEq, Eq)]
enum WorldAccess {
    Unobserved,
    Synchronized,
    Mutable,
}

impl<'a> ExecutionContext<'a> {
    pub(super) fn new(
        plot: &'a mut Plot,
        player_idx: usize,
        arguments: &'a [ParsedArgument<Value>],
    ) -> Self {
        Self {
            plot,
            player_idx,
            arguments,
            world_access: WorldAccess::Unobserved,
        }
    }

    fn value(&self, name: &str) -> Option<&'a Value> {
        self.arguments
            .iter()
            .find(|argument| argument.name == name)
            .map(|argument| &argument.value)
    }

    pub(super) fn arg<T: FromValue<'a>>(&self, name: &str) -> CommandResult<T> {
        self.arg_opt(name)?.ok_or_else(|| {
            InternalError::MissingArgument {
                name: name.to_string(),
            }
            .into()
        })
    }

    pub(super) fn arg_opt<T: FromValue<'a>>(&self, name: &str) -> CommandResult<Option<T>> {
        let Some(value) = self.value(name) else {
            return Ok(None);
        };
        match T::from_value(value) {
            Some(value) => Ok(Some(value)),
            None => Err(InternalError::WrongArgumentType {
                name: name.to_string(),
                expected: T::TYPE_NAME,
                found: format!("{value:?}"),
            }
            .into()),
        }
    }

    pub(super) fn arg_or<T: FromValue<'a>>(&self, name: &str, default: T) -> CommandResult<T> {
        Ok(self.arg_opt(name)?.unwrap_or(default))
    }

    pub(super) fn flag(&self, long_name: &str) -> bool {
        match self.value(FLAGS_ARGUMENT) {
            Some(Value::Flags(flags)) => flags.contains(long_name),
            _ => false,
        }
    }

    pub(super) fn position(&self, name: &str) -> CommandResult<Option<PlayerPos>> {
        let Some(coordinates) = self.arg_opt::<PositionCoordinates>(name)? else {
            return Ok(None);
        };
        let player = self.player()?;
        let position = player.pos;
        let (x, y, z) = coordinates.resolve(
            (position.x, position.y, position.z),
            player.yaw,
            player.pitch,
        )?;
        Ok(Some(PlayerPos::new(x, y, z)))
    }

    pub(super) fn block_position(&self, name: &str) -> CommandResult<Option<BlockPos>> {
        let Some(coordinates) = self.arg_opt::<BlockCoordinates>(name)? else {
            return Ok(None);
        };
        let player = self.player()?;
        let position = player.pos;
        Ok(Some(coordinates.resolve(
            (position.x, position.y, position.z),
            player.yaw,
            player.pitch,
        )?))
    }

    pub(super) fn reply(&self, message: &str) -> CommandResult<()> {
        self.player()?.send_system_message(message);
        Ok(())
    }

    pub(super) fn reply_legacy(&self, message: &str) -> CommandResult<()> {
        let components = TextComponent::from_legacy_text(message);
        self.player()?.send_chat_message(&components);
        Ok(())
    }

    pub(super) fn plot_mut(&mut self) -> &mut Plot {
        self.plot
    }

    pub(super) fn world(&mut self) -> &PlotWorld {
        if self.world_access == WorldAccess::Unobserved {
            if self.plot.redpiler.is_active() {
                self.plot.redpiler.flush_all(&mut self.plot.world);
            }
            self.world_access = WorldAccess::Synchronized;
        }
        &self.plot.world
    }

    pub(super) fn world_mut(&mut self) -> &mut PlotWorld {
        if self.world_access != WorldAccess::Mutable {
            self.world();
            self.plot.reset_redpiler();
            self.world_access = WorldAccess::Mutable;
        }
        &mut self.plot.world
    }

    pub(super) fn player(&self) -> Result<&Player, InternalError> {
        let index = self.player_idx;
        self.plot
            .players
            .get(index)
            .ok_or(InternalError::InvalidPlayerIndex { index })
    }

    pub(super) fn player_mut(&mut self) -> Result<&mut Player, InternalError> {
        let index = self.player_idx;
        self.plot
            .players
            .get_mut(index)
            .ok_or(InternalError::InvalidPlayerIndex { index })
    }

    pub(super) fn player_index(&self) -> usize {
        self.player_idx
    }

    pub(super) fn target_block(&mut self, max_distance: f64) -> CommandResult<BlockPos> {
        let player = self.player()?;
        let (pos, pitch, yaw) = (player.pos, player.pitch as f64, player.yaw as f64);
        ray_trace_block(self.world(), pos, pitch, yaw, max_distance)
            .ok_or_else(|| RuntimeError::NoBlockInSight.into())
    }

    pub(super) fn get_selection(&mut self) -> CommandResult<(BlockPos, BlockPos)> {
        let plot_x = self.plot.world.x;
        let plot_z = self.plot.world.z;
        let player = self.player()?;

        let (first, second) = match (player.worldedit_first_pos(), player.worldedit_second_pos()) {
            (Some(first), Some(second)) => (first, second),
            _ => return Err(RuntimeError::NoSelection.into()),
        };

        for (position, pos) in [("First", first), ("Second", second)] {
            if !Plot::in_plot_bounds(plot_x, plot_z, pos.x, pos.z)
                || !(0..PLOT_BLOCK_HEIGHT).contains(&pos.y)
            {
                return Err(RuntimeError::SelectionOutOfBounds {
                    position: position.to_string(),
                }
                .into());
            }
        }

        Ok((first, second))
    }

    pub(super) fn clip_region(
        &self,
        first: BlockPos,
        second: BlockPos,
    ) -> Option<(BlockPos, BlockPos)> {
        let (plot_min, plot_max) = self.plot.world.get_corners();
        let min = first.min(second).max(plot_min);
        let max = first.max(second).min(plot_max);
        (min.x <= max.x && min.y <= max.y && min.z <= max.z).then_some((min, max))
    }

    pub(super) fn capture_undo_regions(
        &mut self,
        regions: impl IntoIterator<Item = (BlockPos, BlockPos)>,
        origin: BlockPos,
    ) -> CommandResult<bool> {
        self.player()?;
        let mut clipped = false;
        let regions: Vec<_> = regions
            .into_iter()
            .filter_map(|(first, second)| {
                let region = self.clip_region(first, second);
                clipped |= region != Some((first.min(second), first.max(second)));
                region
            })
            .collect();
        if regions.is_empty() {
            return Err(RuntimeError::DestinationOutsidePlot.into());
        }
        let world = self.world_mut();
        let clipboards = regions
            .into_iter()
            .map(|(first_pos, second_pos)| create_clipboard(world, origin, first_pos, second_pos))
            .collect();

        let undo = WorldEditUndo {
            clipboards,
            pos: origin,
            plot_x: world.x,
            plot_z: world.z,
        };

        let player = self.player_mut()?;
        player.worldedit_undo.push(undo);
        player.worldedit_redo.clear();
        Ok(clipped)
    }

    pub(super) fn worldedit_message(&mut self, message: &str) -> CommandResult<()> {
        self.player()?.send_worldedit_message(message);
        Ok(())
    }

    pub(super) fn get_clipboard(&self) -> CommandResult<&WorldEditClipboard> {
        self.player()?
            .worldedit_clipboard
            .as_ref()
            .ok_or_else(|| RuntimeError::EmptyClipboard.into())
    }

    pub(super) fn set_clipboard(&mut self, clipboard: WorldEditClipboard) -> CommandResult<()> {
        self.player_mut()?.worldedit_clipboard = Some(clipboard);
        Ok(())
    }
}
