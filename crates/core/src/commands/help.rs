use crate::commands::{argument::FlagSpec, node::Policy, registry::CommandRegistry};
use itertools::Itertools;
use mchprs_commands::{NodeId, NodeKind};

pub(super) struct CommandHelp {
    pub usage: String,
    pub flags: Vec<FlagSpec>,
}

#[derive(Default)]
pub struct Help<'a> {
    pub usages: Vec<String>,
    pub flags: Vec<&'a FlagSpec>,
}

pub fn generate_help<'a>(
    registry: &'a CommandRegistry,
    path: &[NodeId],
    allowed: &impl Fn(&Policy) -> bool,
) -> Help<'a> {
    fn collect<'a>(
        registry: &'a CommandRegistry,
        id: NodeId,
        prefix: String,
        allowed: &impl Fn(&Policy) -> bool,
        output: &mut Help<'a>,
    ) {
        let id = registry.graph.node(id).redirect().unwrap_or(id);
        let node = registry.graph.node(id);
        if !allowed(&node.metadata) {
            return;
        }
        if let Some(help) = registry.help.get(&id) {
            output
                .usages
                .push(format!("{prefix} {}", help.usage).trim_end().to_owned());
            output.flags.extend(&help.flags);
        }
        for &child in node.children() {
            let node = registry.graph.node(child);
            if matches!(node.kind, NodeKind::Literal(_)) && node.redirect().is_none() {
                collect(
                    registry,
                    child,
                    format!("{prefix} {}", node.name()),
                    allowed,
                    output,
                );
            }
        }
    }

    let mut help = Help::default();
    if let Some(&last) = path.last() {
        let prefix = format!(
            "/{}",
            path.iter()
                .map(|&id| registry.graph.node(id).name())
                .join(" ")
        );
        collect(registry, last, prefix, allowed, &mut help);
    }
    help.flags = help.flags.into_iter().unique().collect();
    help
}
