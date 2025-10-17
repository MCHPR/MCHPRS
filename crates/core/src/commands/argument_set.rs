use crate::{
    commands::{
        error::{CommandResult, InternalError},
        value::{ColumnPos, Direction, DirectionExt, Value, Vec3},
    },
    worldedit::WorldEditPattern,
};
use rustc_hash::{FxHashMap, FxHashSet};

pub struct ArgumentSet {
    args: FxHashMap<String, Value>,
}

impl ArgumentSet {
    pub(super) fn empty() -> Self {
        Self {
            args: FxHashMap::default(),
        }
    }

    pub(super) fn new(args: Vec<(String, Value)>) -> Self {
        Self {
            args: args.into_iter().collect(),
        }
    }

    fn get(&self, name: &str) -> CommandResult<&Value> {
        self.args.get(name).ok_or_else(|| {
            InternalError::MissingArgument {
                name: name.to_string(),
            }
            .into()
        })
    }

    pub fn get_string(&self, name: &str) -> CommandResult<String> {
        Ok(self.get(name)?.as_string()?.clone())
    }

    pub fn get_integer(&self, name: &str) -> CommandResult<i32> {
        self.get(name)?.as_integer()
    }

    pub fn get_float(&self, name: &str) -> CommandResult<f32> {
        self.get(name)?.as_float()
    }

    pub fn get_boolean(&self, name: &str) -> CommandResult<bool> {
        self.get(name)?.as_boolean()
    }

    pub fn get_player(&self, name: &str) -> CommandResult<String> {
        Ok(self.get(name)?.as_player()?.clone())
    }

    pub fn get_vec3(&self, name: &str) -> CommandResult<Vec3> {
        self.get(name)?.as_vec3()
    }

    pub fn get_column_pos(&self, name: &str) -> CommandResult<ColumnPos> {
        self.get(name)?.as_column_pos()
    }

    pub fn get_container(
        &self,
        name: &str,
    ) -> CommandResult<mchprs_blocks::block_entities::ContainerType> {
        self.get(name)?.as_container()
    }

    pub fn get_pattern(&self, name: &str) -> CommandResult<WorldEditPattern> {
        Ok(self.get(name)?.as_pattern()?.clone())
    }

    pub fn get_mask(&self, name: &str) -> CommandResult<WorldEditPattern> {
        Ok(self.get(name)?.as_mask()?.clone())
    }

    pub fn get_direction(&self, name: &str) -> CommandResult<Direction> {
        self.get(name)?.as_direction()
    }

    pub fn get_direction_ext(&self, name: &str) -> CommandResult<DirectionExt> {
        self.get(name)?.as_direction_ext()
    }

    pub fn get_block_pos(&self, name: &str) -> CommandResult<mchprs_blocks::BlockPos> {
        self.get(name)?.as_block_pos()
    }

    pub fn get_greedy(&self, name: &str) -> CommandResult<String> {
        Ok(self.get(name)?.as_greedy()?.clone())
    }

    pub fn get_flags(&self, name: &str) -> CommandResult<FxHashSet<String>> {
        Ok(self.get(name)?.as_flags()?.clone())
    }
}
