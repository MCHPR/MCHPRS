mod core;
mod worldedit;

use crate::commands::{
    argument::ArgumentType, context::ExecutionContext, define::Command, error::CommandResult,
    help::generate_help, node::Policy, registry::CommandRegistry, COMMAND_REGISTRY,
};
use mchprs_commands::NodeKind;

pub fn register_commands(registry: &mut CommandRegistry) {
    core::register_commands(registry);
    worldedit::register_commands(registry);
    registry.register(
        Command::new("help")
            .optional("command", ArgumentType::GreedyString)
            .executes(exec_help),
    );
    registry.register(
        Command::new("/help")
            .permission("worldedit.help")
            .require_plot_ownership()
            .optional("command", ArgumentType::GreedyString)
            .executes(exec_help),
    );
}

fn exec_help(ctx: &mut ExecutionContext<'_>) -> CommandResult<()> {
    let command: String = ctx.arg_or("command", String::new())?;
    let registry = &*COMMAND_REGISTRY;
    let player = ctx.player()?;
    let allowed = |policy: &Policy| policy.allows(player);
    if command.is_empty() {
        let mut names: Vec<_> = registry
            .visible_commands(&allowed)
            .map(|(name, _)| format!("/{name}"))
            .collect();
        names.sort();
        ctx.reply("Run /help <command> for more information.")?;
        return ctx.reply(&names.join(", "));
    }
    let alternate = command
        .strip_prefix('/')
        .map(str::to_owned)
        .unwrap_or_else(|| format!("/{command}"));
    for candidate in [command.as_str(), alternate.as_str()] {
        let expanded = registry.expand(candidate);
        let parsed = registry.graph.parse(&expanded.text, allowed);
        let path = parsed.context.path();
        let literals: Vec<_> = path
            .iter()
            .copied()
            .take_while(|&id| matches!(registry.graph.node(id).kind, NodeKind::Literal(_)))
            .collect();
        if !literals.is_empty() {
            let help = generate_help(registry, &literals, &allowed);
            if help.usages.is_empty() {
                continue;
            }
            for usage in help.usages {
                ctx.reply(&format!("Usage: {usage}"))?;
            }
            for flag in help.flags {
                let short = flag
                    .short
                    .map_or(String::new(), |short| format!("-{short}, "));
                ctx.reply(&format!("{short}--{}: {}", flag.long, flag.description))?;
            }
            return Ok(());
        }
    }
    ctx.reply(&format!("Command not found: {command}"))
}
