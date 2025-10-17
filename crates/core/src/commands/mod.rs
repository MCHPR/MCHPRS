mod argument;
mod argument_parser;
mod argument_set;
mod autocomplete;
mod builtin;
mod context;
mod error;
mod executor;
mod node;
mod parser;
mod registry;
mod usage;
mod value;

use crate::config::CONFIG;
use once_cell::sync::Lazy;
use registry::CommandRegistry;

pub static COMMAND_REGISTRY: Lazy<CommandRegistry> = Lazy::new(|| {
    let mut registry = CommandRegistry::new();
    builtin::register_commands(&mut registry);
    for (alias, replacement) in &CONFIG.command_aliases {
        registry.add_custom_alias(alias.trim(), replacement.trim());
    }
    registry.rebuild_declare_commands_packet();
    registry
});

pub enum CommandSender {
    Player(usize),
    Console,
}
