use crate::{Reader, SyntaxError};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_GRAPH_ID: AtomicU64 = AtomicU64::new(1);

pub trait Argument: Clone + PartialEq {
    type Value: Clone;

    fn consumes_remaining(&self) -> bool {
        false
    }

    fn parse(&self, reader: &mut Reader<'_>) -> Result<Self::Value, SyntaxError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    graph: u64,
    index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind<A> {
    Root,
    Literal(String),
    Argument { name: String, parser: A },
}

impl<A> NodeKind<A> {
    pub fn name(&self) -> &str {
        match self {
            Self::Root => "",
            Self::Literal(name) | Self::Argument { name, .. } => name,
        }
    }
}

impl<A: Argument> NodeKind<A> {
    pub(crate) fn parse(&self, reader: &mut Reader<'_>) -> Result<Option<A::Value>, SyntaxError> {
        let start = reader.cursor();
        let value = match self {
            Self::Literal(name) => {
                if reader.take_while(|ch| ch != ' ') != name {
                    return Err(SyntaxError::new(start, format!("Expected {name}")));
                }
                None
            }
            Self::Argument { parser, .. } => Some(parser.parse(reader)?),
            Self::Root => unreachable!(),
        };
        if reader.peek().is_some_and(|ch| ch != ' ') {
            return Err(reader.error("Expected whitespace between arguments"));
        }
        Ok(value)
    }
}

pub struct Node<A, E, M> {
    pub kind: NodeKind<A>,
    pub executor: Option<E>,
    pub metadata: M,
    pub(crate) children: Vec<NodeId>,
    pub(crate) redirect: Option<NodeId>,
}

impl<A, E, M> Node<A, E, M> {
    pub fn new(kind: NodeKind<A>, executor: Option<E>, metadata: M) -> Self {
        Self {
            kind,
            executor,
            metadata,
            children: Vec::new(),
            redirect: None,
        }
    }

    pub fn name(&self) -> &str {
        self.kind.name()
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn redirect(&self) -> Option<NodeId> {
        self.redirect
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("Invalid command node name: {0:?}")]
    InvalidName(String),
    #[error("Repeated argument name in one context: {0}")]
    RepeatedArgument(String),
    #[error("Incompatible command definitions for {0}")]
    Conflict(String),
    #[error("A redirect cannot also have children")]
    RedirectWithChildren,
    #[error("A greedy argument cannot have children or a redirect")]
    GreedyContinuation,
    #[error("Invalid command node")]
    InvalidNode,
}

pub struct Graph<A, E, M> {
    id: u64,
    pub(crate) nodes: Vec<Node<A, E, M>>,
}

impl<A, E, M> Graph<A, E, M> {
    pub fn new(metadata: M) -> Self {
        Self {
            id: NEXT_GRAPH_ID.fetch_add(1, Ordering::Relaxed),
            nodes: vec![Node::new(NodeKind::Root, None, metadata)],
        }
    }

    pub fn root(&self) -> NodeId {
        NodeId {
            graph: self.id,
            index: 0,
        }
    }

    pub fn node(&self, id: NodeId) -> &Node<A, E, M> {
        assert_eq!(id.graph, self.id, "Node belongs to a different graph");
        &self.nodes[id.index]
    }

    pub fn set_executor(&mut self, id: NodeId, executor: E) {
        assert_eq!(id.graph, self.id, "Node belongs to a different graph");
        self.nodes[id.index].executor = Some(executor);
    }

    pub fn child(&self, parent: NodeId, name: &str) -> Option<NodeId> {
        self.node(parent)
            .children
            .iter()
            .copied()
            .find(|&id| self.node(id).name() == name)
    }

    pub(crate) fn matching_literal(&self, parent: NodeId, input: &str) -> Option<NodeId> {
        let token = input.split(' ').next().unwrap_or_default();
        self.node(parent)
            .children()
            .iter()
            .copied()
            .find(|&id| matches!(&self.node(id).kind, NodeKind::Literal(name) if name == token))
    }
}

impl<A: Argument, E, M: PartialEq> Graph<A, E, M> {
    pub fn insert(
        &mut self,
        parent: NodeId,
        node: Node<A, E, M>,
    ) -> Result<NodeId, RegistrationError> {
        if parent.graph != self.id {
            return Err(RegistrationError::InvalidNode);
        }

        let Some(parent_node) = self.nodes.get(parent.index) else {
            return Err(RegistrationError::InvalidNode);
        };
        if matches!(&parent_node.kind, NodeKind::Argument { parser, .. } if parser.consumes_remaining())
        {
            return Err(RegistrationError::GreedyContinuation);
        }
        if parent_node.redirect.is_some() {
            return Err(RegistrationError::RedirectWithChildren);
        }
        if matches!(node.kind, NodeKind::Root)
            || node.name().is_empty()
            || node.name().chars().any(char::is_whitespace)
        {
            return Err(RegistrationError::InvalidName(node.name().to_owned()));
        }

        if let Some(id) = self.child(parent, node.name()) {
            let existing = &mut self.nodes[id.index];
            if existing.kind != node.kind
                || existing.metadata != node.metadata
                || existing.redirect.is_some()
            {
                return Err(RegistrationError::Conflict(node.name().to_owned()));
            }
            if node.executor.is_some() {
                existing.executor = node.executor;
            }
            return Ok(id);
        }

        let id = NodeId {
            graph: self.id,
            index: self.nodes.len(),
        };
        self.nodes.push(node);
        self.nodes[parent.index].children.push(id);
        Ok(id)
    }

    pub fn redirect(&mut self, id: NodeId, target: NodeId) -> Result<(), RegistrationError> {
        if id.graph != self.id
            || target.graph != self.id
            || id == self.root()
            || target.index >= self.nodes.len()
        {
            return Err(RegistrationError::InvalidNode);
        }

        let node = self
            .nodes
            .get_mut(id.index)
            .ok_or(RegistrationError::InvalidNode)?;
        if node.redirect.is_some_and(|existing| existing != target) {
            return Err(RegistrationError::Conflict(node.name().to_owned()));
        }
        if matches!(&node.kind, NodeKind::Argument { parser, .. } if parser.consumes_remaining()) {
            return Err(RegistrationError::GreedyContinuation);
        }
        if !node.children.is_empty() {
            return Err(RegistrationError::RedirectWithChildren);
        }

        node.redirect = Some(target);
        Ok(())
    }
}
