use crate::{
    commands::{
        argument_set::ArgumentSet,
        context::ExecutionContext,
        error::{CommandError, CommandResult, InternalError, UnwrapRuntimeError},
        node::CommandNode,
        parser::{self, ParseResult},
        registry::CommandRegistry,
        usage, CommandSender,
    },
    plot::Plot,
};

impl CommandRegistry {
    pub fn execute(
        &self,
        plot: &mut Plot,
        sender: CommandSender,
        command_line: &str,
    ) -> Result<(), InternalError> {
        let command_line = command_line.trim();
        if command_line.is_empty() {
            return Ok(());
        }

        let command_line = self.expand_custom_aliases(command_line);
        let parse_result = parser::parse(self.get_root(), &command_line);

        match parse_result {
            ParseResult::Success {
                node,
                arguments,
                path,
            } => {
                let mut ctx = ExecutionContext::new(plot, sender, ArgumentSet::new(arguments));
                match Self::execute_impl(&mut ctx, node, &path) {
                    Ok(()) => Ok(()),
                    Err(CommandError::Runtime(err)) => {
                        ctx.error(&err.to_string()).unwrap_runtime()?;
                        Ok(())
                    }
                    Err(CommandError::Internal(err)) => Err(err),
                }
            }

            ParseResult::Partial { path, .. } => {
                Self::handle_parse_error(plot, sender, &path, "Not enough arguments")
            }
            ParseResult::TooManyArguments {
                path, remaining, ..
            } => {
                let message = format!("Too many arguments: {remaining}");

                Self::handle_parse_error(plot, sender, &path, &message)
            }
            ParseResult::InvalidArgument {
                path, remaining, ..
            } => {
                let message = format!("Invalid argument: {remaining}");
                Self::handle_parse_error(plot, sender, &path, &message)
            }

            ParseResult::NothingMatched { .. } => {
                let ctx = ExecutionContext::new(plot, sender, ArgumentSet::empty());
                ctx.error("Command not found!").unwrap_runtime()?;
                Ok(())
            }
        }
    }

    fn execute_impl<'a>(
        ctx: &mut ExecutionContext<'a>,
        node: &CommandNode,
        path: &[&CommandNode],
    ) -> CommandResult<()> {
        let Some(executor) = node.executor else {
            unreachable!("Node must have executor to be parsed successfully")
        };

        for node in path {
            for permission in &node.permissions {
                ctx.require_permission(permission)?;
            }
        }

        if path.iter().any(|node| node.requires_plot_ownership) {
            ctx.require_plot_ownership()?;
        }

        if path.iter().any(|node| node.mutates_world) {
            ctx.plot.reset_redpiler();
        }

        executor(ctx)
    }

    fn expand_custom_aliases(&self, command_line: &str) -> String {
        for (alias, expansion) in self.get_custom_aliases() {
            if let Some(rest) = command_line.strip_prefix(alias) {
                if expansion.contains("{}") {
                    return expansion.replace("{}", rest);
                } else {
                    return format!("{} {}", expansion, rest);
                }
            }
        }

        command_line.to_string()
    }

    fn handle_parse_error(
        plot: &mut Plot,
        sender: CommandSender,
        path: &[&CommandNode],
        error_message: &str,
    ) -> Result<(), InternalError> {
        let ctx = ExecutionContext::new(plot, sender, ArgumentSet::empty());

        ctx.error(error_message).unwrap_runtime()?;

        let usage = usage::generate_usage(path);
        ctx.reply_legacy(&format!("&6Usage: &e{}", usage))
            .unwrap_runtime()?;
        let base_name = usage::generate_base_name(path);
        ctx.reply_legacy(&format!(
            "&eRun &e/help {}&e for more information.",
            base_name
        ))
        .unwrap_runtime()?;

        Ok(())
    }
}
