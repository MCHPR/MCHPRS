use crate::commands::{
    argument::{ArgumentType, FlagSpec},
    help::CommandHelp,
    node::{Executor, Policy, RegisteredNode},
    registry::CommandRegistry,
};
use mchprs_commands::{NodeId, NodeKind, RegistrationError};

pub const FLAGS_ARGUMENT: &str = "flags";

pub struct Command {
    name: String,
    aliases: Vec<String>,
    policy: Policy,
    subcommands: Vec<Command>,
    syntax: Vec<Syntax>,
    flags: Vec<FlagSpec>,
    executor: Option<Executor>,
}

pub type Syntax = mchprs_commands::Syntax<ArgumentType>;

pub fn arg(name: &str, arg_type: ArgumentType) -> Syntax {
    assert!(
        name != FLAGS_ARGUMENT,
        "{FLAGS_ARGUMENT} is a reserved name"
    );
    Syntax::Argument(name.to_owned(), arg_type)
}

pub fn opt<const N: usize>(items: [Syntax; N]) -> Syntax {
    Syntax::Optional(items.into())
}

pub fn alt<const N: usize>(alternatives: [Syntax; N]) -> Syntax {
    assert!(N > 0, "empty alternatives");
    Syntax::Alternatives(alternatives.into())
}

impl Command {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            aliases: Vec::new(),
            policy: Policy::default(),
            subcommands: Vec::new(),
            syntax: Vec::new(),
            flags: Vec::new(),
            executor: None,
        }
    }

    pub fn alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_owned());
        self
    }

    pub fn permission(mut self, permission: &str) -> Self {
        self.policy.permissions.push(permission.to_owned());
        self
    }

    pub fn require_plot_ownership(mut self) -> Self {
        self.policy.requires_plot_ownership = true;
        self
    }

    pub fn subcommand(mut self, command: Command) -> Self {
        self.subcommands.push(command);
        self
    }

    pub fn arg(mut self, name: &str, arg_type: ArgumentType) -> Self {
        self.syntax.push(arg(name, arg_type));
        self
    }

    pub fn optional(mut self, name: &str, arg_type: ArgumentType) -> Self {
        self.syntax.push(opt([arg(name, arg_type)]));
        self
    }

    pub fn syntax(mut self, syntax: Syntax) -> Self {
        self.syntax.push(syntax);
        self
    }

    pub fn flag(mut self, short: impl Into<Option<char>>, long: &str, description: &str) -> Self {
        self.flags.push(FlagSpec {
            short: short.into(),
            long: long.to_owned(),
            description: description.to_owned(),
        });
        self
    }

    pub fn executes(mut self, executor: Executor) -> Self {
        self.executor = Some(executor);
        self
    }

    pub(super) fn register(
        mut self,
        registry: &mut CommandRegistry,
        parent: NodeId,
    ) -> Result<NodeId, RegistrationError> {
        if registry.graph.child(parent, &self.name).is_some() {
            return Err(RegistrationError::Conflict(self.name));
        }
        let id = registry.graph.insert(
            parent,
            RegisteredNode::new(NodeKind::Literal(self.name), None, self.policy.clone()),
        )?;
        for command in self.subcommands {
            command.register(registry, id)?;
        }

        if let Some(executor) = self.executor {
            if !self.flags.is_empty() {
                self.syntax.push(opt([Syntax::Argument(
                    FLAGS_ARGUMENT.to_owned(),
                    ArgumentType::Flags {
                        flags: self.flags.clone(),
                    },
                )]));
            }
            let syntax = Syntax::Sequence(self.syntax);
            registry.help.insert(
                id,
                CommandHelp {
                    usage: syntax.to_string(),
                    flags: self.flags,
                },
            );

            for terminal in syntax.register(&mut registry.graph, id)? {
                registry.graph.set_executor(terminal, executor);
            }
        } else {
            assert!(
                self.syntax.is_empty() && self.flags.is_empty(),
                "command {} has arguments but no executor",
                registry.graph.node(id).name()
            );
        }

        for alias in self.aliases {
            if registry.graph.child(parent, &alias).is_some() {
                return Err(RegistrationError::Conflict(alias));
            }
            let executor = registry.graph.node(id).executor;
            let alias_id = registry.graph.insert(
                parent,
                RegisteredNode::new(NodeKind::Literal(alias), executor, self.policy.clone()),
            )?;
            registry.graph.redirect(alias_id, id)?;
        }

        Ok(id)
    }
}
