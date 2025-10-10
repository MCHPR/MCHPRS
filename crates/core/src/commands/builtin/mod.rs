mod core;
mod worldedit;

use crate::commands::{
    argument::ArgumentType,
    context::ExecutionContext,
    error::CommandResult,
    node::{CommandNode, NodeType},
    parser,
    registry::CommandRegistry,
    usage, COMMAND_REGISTRY,
};

pub fn register_commands(registry: &mut CommandRegistry) {
    core::register_commands(registry);
    worldedit::register_commands(registry);

    register_help(registry);
}

fn register_help(registry: &mut CommandRegistry) {
    fn exec_help_command(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
        let command = ctx.args().get_greedy("command")?;
        let command = command.trim_start();
        let root = COMMAND_REGISTRY.get_root();

        if command.is_empty() {
            let mut commands = Vec::new();

            for child in &root.children {
                if let NodeType::Literal { name, aliases } = &child.node_type {
                    if aliases.is_empty() {
                        commands.push(format!("/{}", name));
                    } else {
                        commands.push(format!("/{} (Aliases: {})", name, aliases.join(", ")));
                    }
                }
            }

            commands.sort();
            ctx.reply("Run /help <command> for more information.")?;
            ctx.reply("Available commands:")?;
            for cmd in commands {
                ctx.reply(&format!(" {}", cmd))?;
            }
        } else {
            match parser::parse(root, command) {
                parser::ParseResult::Success { path, .. }
                | parser::ParseResult::Partial { path, .. }
                | parser::ParseResult::TooManyArguments { path, .. }
                | parser::ParseResult::InvalidArgument { path, .. } => {
                    let usage = usage::generate_usage(&path);
                    ctx.reply(&format!("Usage: {}", usage))?;
                    let flag_details = usage::generate_flag_details(path.last().unwrap());
                    if !flag_details.is_empty() {
                        ctx.reply("Available flags:")?;
                        for flag_detail in flag_details {
                            ctx.reply(&format!(" {flag_detail}"))?;
                        }
                    }
                }
                parser::ParseResult::NothingMatched { .. } => {
                    ctx.reply(&format!("Command not found: {}", command))?;
                }
            }
        }
        Ok(())
    }

    registry.register(CommandNode::literal("help").then(
        CommandNode::argument("command", ArgumentType::greedy_string()).executes(exec_help_command),
    ));
}
