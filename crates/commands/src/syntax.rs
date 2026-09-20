use crate::{Argument, Graph, Node, NodeId, NodeKind, RegistrationError};
use std::fmt;

pub enum Syntax<A> {
    Argument(String, A),
    Sequence(Vec<Syntax<A>>),
    Optional(Vec<Syntax<A>>),
    Alternatives(Vec<Syntax<A>>),
}

impl<A: Argument> Syntax<A> {
    pub fn register<E, M: Default + PartialEq>(
        self,
        graph: &mut Graph<A, E, M>,
        parent: NodeId,
    ) -> Result<Vec<NodeId>, RegistrationError> {
        let mut registered = Vec::new();
        for path in self.paths() {
            let mut terminal = parent;
            for (index, &(name, parser)) in path.iter().enumerate() {
                if path[..index].iter().any(|&(previous, _)| previous == name) {
                    return Err(RegistrationError::RepeatedArgument(name.to_owned()));
                }
                terminal = graph.insert(
                    terminal,
                    Node::new(
                        NodeKind::Argument {
                            name: name.to_owned(),
                            parser: parser.clone(),
                        },
                        None,
                        M::default(),
                    ),
                )?;
            }
            registered.push(terminal);
        }

        Ok(registered)
    }

    fn paths(&self) -> Vec<Vec<(&str, &A)>> {
        match self {
            Self::Argument(name, parser) => vec![vec![(name, parser)]],
            Self::Sequence(items) | Self::Optional(items) => {
                let mut paths = vec![Vec::new()];
                for item in items {
                    let suffixes = item.paths();
                    paths = paths
                        .into_iter()
                        .flat_map(|prefix| {
                            suffixes
                                .iter()
                                .map(move |suffix| prefix.iter().chain(suffix).copied().collect())
                        })
                        .collect();
                }
                if matches!(self, Self::Optional(_)) {
                    paths.push(Vec::new());
                }
                paths
            }
            Self::Alternatives(items) => items.iter().flat_map(Self::paths).collect(),
        }
    }
}

impl<A> fmt::Display for Syntax<A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (items, opening, separator, closing) = match self {
            Self::Argument(name, _) => return write!(formatter, "<{name}>"),
            Self::Sequence(items) => (items, "", " ", ""),
            Self::Optional(items) => (items, "[", " ", "]"),
            Self::Alternatives(items) => (items, "(", " | ", ")"),
        };

        formatter.write_str(opening)?;
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                formatter.write_str(separator)?;
            }
            write!(formatter, "{item}")?;
        }
        formatter.write_str(closing)
    }
}
