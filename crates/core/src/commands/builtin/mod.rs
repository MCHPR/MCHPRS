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
                    ctx.reply_legacy(&format!("&6Usage: &e{}", usage))?;
                    let flag_details = usage::generate_flag_details(path.last().unwrap());
                    if !flag_details.is_empty() {
                        ctx.reply_legacy("&6Available flags:")?;
                        for flag in flag_details {
                            let flag_part = match flag.short {
                                Some(short) => format!("&a-{} &e| &a--{}", short, flag.long),
                                None => format!("&a--{}", flag.long),
                            };
                            let flag_details = match &flag.description {
                                Some(desc) => format!(" {}&e: {}", flag_part, desc),
                                None => format!(" {}", flag_part),
                            };
                            ctx.reply_legacy(&flag_details)?;
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
