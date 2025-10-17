use crate::commands::{argument::ArgumentType, context::ExecutionContext, error::CommandResult};

pub type Executor = fn(&mut ExecutionContext<'_>) -> CommandResult<()>;

#[derive(Clone)]
pub struct CommandNode {
    pub(super) node_type: NodeType,
    pub(super) children: Vec<CommandNode>,
    pub(super) executor: Option<Executor>,
    pub(super) permissions: Vec<String>,
    pub(super) requires_plot_ownership: bool,
    pub(super) mutates_world: bool,
}

#[derive(Clone)]
pub enum NodeType {
    Root,
    Literal {
        name: String,
        aliases: Vec<String>,
    },
    Argument {
        name: String,
        arg_type: ArgumentType,
    },
}

impl CommandNode {
    pub fn root() -> Self {
        Self {
            node_type: NodeType::Root,
            children: Vec::new(),
            executor: None,
            permissions: Vec::new(),
            requires_plot_ownership: false,
            mutates_world: false,
        }
    }

    pub fn literal(name: impl Into<String>) -> Self {
        Self {
            node_type: NodeType::Literal {
                name: name.into(),
                aliases: Vec::new(),
            },
            children: Vec::new(),
            executor: None,
            permissions: Vec::new(),
            requires_plot_ownership: false,
            mutates_world: false,
        }
    }

    pub fn argument(name: impl Into<String>, arg_type: impl Into<ArgumentType>) -> Self {
        Self {
            node_type: NodeType::Argument {
                name: name.into(),
                arg_type: arg_type.into(),
            },
            children: Vec::new(),
            executor: None,
            permissions: Vec::new(),
            requires_plot_ownership: false,
            mutates_world: false,
        }
    }

    pub fn then(mut self, child: CommandNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn executes(mut self, executor: Executor) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        let NodeType::Literal { aliases, .. } = &mut self.node_type else {
            panic!("Can not create alias for non-literal command nodes")
        };
        aliases.push(alias.into());
        self
    }

    pub fn require_permission(mut self, permission: impl Into<String>) -> Self {
        self.permissions.push(permission.into());
        self
    }

    pub fn require_plot_ownership(mut self) -> Self {
        self.requires_plot_ownership = true;
        self
    }

    pub fn mutates_world(mut self) -> Self {
        self.mutates_world = true;
        self
    }

    pub(super) fn has_executor(&self) -> bool {
        self.executor.is_some()
    }
}
