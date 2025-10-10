use super::{argument_set::ArgumentSet, CommandSender};
use crate::commands::error::{CommandResult, InternalError, RuntimeError};
use crate::player::{PacketSender, Player};
use crate::plot::{Plot, PlotWorld};
use crate::worldedit::{create_clipboard, WorldEditClipboard, WorldEditUndo};
use mchprs_blocks::BlockPos;

pub struct ExecutionContext<'a> {
    pub plot: &'a mut Plot,
    sender: CommandSender,
    arguments: ArgumentSet,
}

impl<'a> ExecutionContext<'a> {
    pub(super) fn new(plot: &'a mut Plot, sender: CommandSender, arguments: ArgumentSet) -> Self {
        Self {
            plot,
            sender,
            arguments,
        }
    }

    pub fn args(&self) -> &ArgumentSet {
        &self.arguments
    }

    pub fn reply(&self, message: &str) -> CommandResult<()> {
        match &self.sender {
            CommandSender::Player(_) => {
                self.player()?.send_system_message(message);
            }
            CommandSender::Console => {
                println!("{}", message);
            }
        }
        Ok(())
    }

    pub fn error(&self, message: &str) -> CommandResult<()> {
        match &self.sender {
            CommandSender::Player(_) => {
                self.player()?.send_error_message(message);
            }
            CommandSender::Console => {
                eprintln!("Error: {}", message);
            }
        }
        Ok(())
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        match &self.sender {
            CommandSender::Player(_) => {
                if let Ok(player) = self.player() {
                    player.has_permission(permission)
                } else {
                    false
                }
            }
            CommandSender::Console => true,
        }
    }

    pub fn require_permission(&mut self, permission: &str) -> CommandResult<()> {
        if !self.has_permission(permission) {
            return Err(RuntimeError::PermissionDenied {
                permission: permission.to_string(),
            }
            .into());
        }
        Ok(())
    }

    pub fn has_plot_ownership(&self) -> bool {
        match &self.sender {
            CommandSender::Player(_) => {
                if let Ok(player) = self.player() {
                    if player.has_permission("plots.worldedit.bypass") {
                        return true;
                    }
                    if let Some(owner) = self.plot.owner() {
                        owner == player.uuid
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CommandSender::Console => true,
        }
    }

    pub fn require_plot_ownership(&mut self) -> CommandResult<()> {
        if !self.has_plot_ownership() {
            return Err(RuntimeError::PlotOwnershipRequired.into());
        }
        Ok(())
    }

    pub fn world(&self) -> &PlotWorld {
        &self.plot.world
    }

    pub fn world_mut(&mut self) -> &mut PlotWorld {
        &mut self.plot.world
    }

    pub fn player(&self) -> CommandResult<&Player> {
        let index = self.player_index()?;
        self.plot
            .players
            .get(index)
            .ok_or_else(|| InternalError::InvalidPlayerIndex { index }.into())
    }

    pub fn player_mut(&mut self) -> CommandResult<&mut Player> {
        let index = self.player_index()?;
        self.plot
            .players
            .get_mut(index)
            .ok_or_else(|| InternalError::InvalidPlayerIndex { index }.into())
    }

    pub fn player_index(&self) -> CommandResult<usize> {
        match &self.sender {
            CommandSender::Player(index) => Ok(*index),
            CommandSender::Console => Err(RuntimeError::PlayerOnly.into()),
        }
    }
}

impl<'a> ExecutionContext<'a> {
    pub fn get_selection(&mut self) -> CommandResult<(BlockPos, BlockPos)> {
        let plot_x = self.plot.world.x;
        let plot_z = self.plot.world.z;
        let player = self.player()?;

        match (player.first_position, player.second_position) {
            (Some(first), Some(second)) => {
                if !Plot::in_plot_bounds(plot_x, plot_z, first.x, first.z) {
                    return Err(RuntimeError::SelectionOutOfBounds {
                        position: "First".to_string(),
                    }
                    .into());
                }

                if !Plot::in_plot_bounds(plot_x, plot_z, second.x, second.z) {
                    return Err(RuntimeError::SelectionOutOfBounds {
                        position: "Second".to_string(),
                    }
                    .into());
                }

                Ok((first, second))
            }
            _ => Err(RuntimeError::NoSelection.into()),
        }
    }

    pub fn capture_undo_regions(
        &mut self,
        regions: impl IntoIterator<Item = (BlockPos, BlockPos)>,
        origin: BlockPos,
        clear_redo: bool,
    ) -> CommandResult<()> {
        let clipboards: Vec<WorldEditClipboard> = regions
            .into_iter()
            .map(|(first_pos, second_pos)| {
                create_clipboard(&mut self.plot.world, origin, first_pos, second_pos)
            })
            .collect();

        let undo = WorldEditUndo {
            clipboards,
            pos: origin,
            plot_x: self.plot.world.x,
            plot_z: self.plot.world.z,
        };

        let player = self.player_mut()?;
        player.worldedit_undo.push(undo);
        if clear_redo {
            player.worldedit_redo.clear();
        }
        Ok(())
    }

    pub fn worldedit_message(&mut self, message: &str) -> CommandResult<()> {
        self.player()?.send_worldedit_message(message);
        Ok(())
    }

    pub fn get_clipboard(&mut self) -> CommandResult<&WorldEditClipboard> {
        let has_clipboard = self.player()?.worldedit_clipboard.is_some();

        if has_clipboard {
            Ok(self.player()?.worldedit_clipboard.as_ref().unwrap())
        } else {
            Err(RuntimeError::EmptyClipboard.into())
        }
    }

    pub fn set_clipboard(&mut self, clipboard: WorldEditClipboard) -> CommandResult<()> {
        self.player_mut()?.worldedit_clipboard = Some(clipboard);
        Ok(())
    }
}
