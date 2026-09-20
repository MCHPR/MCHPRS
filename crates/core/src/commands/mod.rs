mod argument;
mod argument_parser;
mod argument_suggestions;
mod builtin;
mod context;
mod declaration;
mod define;
mod error;
mod executor;
mod help;
mod node;
mod registry;
mod schematic;
mod suggestions;
mod value;

pub(crate) use schematic::schematic_names;
pub(crate) use suggestions::{CommandSuggestions, SuggestionSource};
pub use value::PlayerTarget;

use self::registry::CommandRegistry;
use crate::config::CONFIG;
use std::sync::LazyLock;

pub(crate) static COMMAND_REGISTRY: LazyLock<CommandRegistry> = LazyLock::new(|| {
    let mut registry = CommandRegistry::new();
    builtin::register_commands(&mut registry);
    for (alias, replacement) in &CONFIG.command_aliases {
        registry.add_custom_alias(alias.trim(), replacement.trim());
    }
    registry
        .validate()
        .unwrap_or_else(|error| panic!("Invalid command aliases: {error}"));
    registry
});
