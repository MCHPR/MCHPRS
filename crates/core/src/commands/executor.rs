use crate::{
    commands::{
        context::ExecutionContext,
        error::{CommandError, InternalError, RuntimeError},
        node::{Executor, Policy},
        registry::CommandRegistry,
        value::Value,
    },
    player::PacketSender,
    plot::Plot,
};
use mchprs_commands::{ParsedArgument, SyntaxError};

pub(super) struct Invocation {
    pub executor: Executor,
    pub arguments: Vec<ParsedArgument<Value>>,
    pub requires_plot_ownership: bool,
}

impl CommandRegistry {
    pub(super) fn dispatch(
        &self,
        input: &str,
        allowed: impl Fn(&Policy) -> bool,
    ) -> Result<Invocation, SyntaxError> {
        let expansion = self.expand(input);
        let parsed = self.graph.parse(&expansion.text, allowed);
        if let Some(mut error) = parsed.error(&expansion.text) {
            error.cursor = expansion.original_cursor(error.cursor);
            return Err(error);
        }
        let requires_plot_ownership = parsed
            .context
            .path()
            .iter()
            .any(|&id| self.graph.node(id).metadata.requires_plot_ownership);
        let context = parsed.context.into_terminal();
        let executor = context
            .executor
            .expect("Successful parse must have an executor");

        Ok(Invocation {
            executor,
            arguments: context.arguments,
            requires_plot_ownership,
        })
    }

    pub(crate) fn execute(
        &self,
        plot: &mut Plot,
        player_idx: usize,
        input: &str,
    ) -> Result<(), InternalError> {
        if input.is_empty() {
            return Ok(());
        }
        let player = plot
            .players
            .get(player_idx)
            .ok_or(InternalError::InvalidPlayerIndex { index: player_idx })?;
        let invocation = match self.dispatch(input, |policy| policy.allows(player)) {
            Ok(invocation) => invocation,
            Err(error) => {
                player.send_error_message(&error.contextual(input));
                return Ok(());
            }
        };
        if invocation.requires_plot_ownership
            && !player.has_permission("plots.worldedit.bypass")
            && plot.owner() != Some(player.uuid)
        {
            player.send_error_message(&RuntimeError::PlotOwnershipRequired.to_string());
            return Ok(());
        }
        let mut context = ExecutionContext::new(plot, player_idx, &invocation.arguments);
        match (invocation.executor)(&mut context) {
            Ok(()) => Ok(()),
            Err(CommandError::Runtime(error)) => {
                context.player()?.send_error_message(&error.to_string());
                Ok(())
            }
            Err(CommandError::Internal(error)) => Err(error),
        }
    }
}
