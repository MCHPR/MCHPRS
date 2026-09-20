use crate::commands::{
    argument::ClientArgument,
    node::{CommandGraph, Policy},
    registry::CommandRegistry,
};
use crate::player::Player;
use mchprs_commands::{NodeId, NodeKind};
use mchprs_network::packets::{
    clientbound::{
        CCommands, CCommandsNode, CDeclareCommandsNodeParser as Parser, ClientBoundPacket,
    },
    PacketEncoder,
};
use rustc_hash::FxHashMap;

const EXECUTABLE: i8 = 4;
const REDIRECT: i8 = 8;
const SUGGESTIONS: i8 = 16;

struct Projection<'a, F> {
    graph: &'a CommandGraph,
    allowed: F,
    nodes: Vec<CCommandsNode>,
    ids: FxHashMap<NodeId, i32>,
}

impl<F: Fn(&Policy) -> bool> Projection<'_, F> {
    fn visit(&mut self, id: NodeId) -> Option<i32> {
        let node = self.graph.node(id);
        if !(self.allowed)(&node.metadata) {
            return None;
        }
        if let Some(&index) = self.ids.get(&id) {
            return Some(index);
        }
        let index = self.nodes.len() as i32;
        self.ids.insert(id, index);
        self.nodes.push(CCommandsNode {
            flags: 0,
            children: Vec::new(),
            redirect_node: None,
            name: None,
            parser: None,
            suggestions_type: None,
        });
        let mut flags = if node.executor.is_some() {
            EXECUTABLE
        } else {
            0
        };
        let mut tail = false;
        let (name, parser, suggestions_type) = match &node.kind {
            NodeKind::Root => (None, None, None),
            NodeKind::Literal(name) => {
                flags |= 1;
                (Some(name.clone()), None, None)
            }
            NodeKind::Argument { name, parser } => {
                flags |= 2;
                let client = parser.client_argument();
                tail = matches!(client, ClientArgument::Opaque);
                let suggests = !matches!(client, ClientArgument::Client(_));
                let protocol = match client {
                    ClientArgument::Opaque => {
                        flags |= EXECUTABLE;
                        Parser::String(2)
                    }
                    ClientArgument::Client(parser) | ClientArgument::AskServer(parser) => parser,
                };
                if suggests {
                    flags |= SUGGESTIONS;
                }
                (
                    Some(name.clone()),
                    Some(protocol),
                    suggests.then(|| "minecraft:ask_server".to_owned()),
                )
            }
        };
        let children: Vec<i32> = if tail {
            Vec::new()
        } else {
            node.children()
                .iter()
                .filter_map(|&child| self.visit(child))
                .collect()
        };
        // The vanilla client cancels its pending suggestion request when another sibling asks the server.
        let mut has_suggestions = false;
        for &child in &children {
            let child = &mut self.nodes[child as usize];
            if child.suggestions_type.is_some() {
                if has_suggestions {
                    child.flags &= !SUGGESTIONS;
                    child.suggestions_type = None;
                }
                has_suggestions = true;
            }
        }
        let redirect_node = node.redirect().and_then(|target| self.visit(target));
        if redirect_node.is_some() {
            flags |= REDIRECT;
        }
        self.nodes[index as usize] = CCommandsNode {
            flags,
            children,
            redirect_node,
            name,
            parser,
            suggestions_type,
        };
        Some(index)
    }
}

impl CommandRegistry {
    pub(super) fn declare_commands(&self, allowed: impl Fn(&Policy) -> bool) -> CCommands {
        let mut projection = Projection {
            graph: &self.graph,
            allowed: &allowed,
            nodes: Vec::new(),
            ids: FxHashMap::default(),
        };
        let root = projection
            .visit(self.graph.root())
            .expect("The root must be accessible");
        let mut children = Vec::new();
        for (name, node) in self.visible_commands(&allowed) {
            if let Some(id) = node {
                children.push(projection.ids[&id]);
                continue;
            }
            let tail = projection.nodes.len() as i32;
            projection.nodes.push(CCommandsNode {
                flags: 2 | EXECUTABLE | SUGGESTIONS,
                children: Vec::new(),
                redirect_node: None,
                name: Some("arguments".to_owned()),
                parser: Some(Parser::String(2)),
                suggestions_type: Some("minecraft:ask_server".to_owned()),
            });
            let index = projection.nodes.len() as i32;
            let executable = self.dispatch(name, &allowed).is_ok();
            projection.nodes.push(CCommandsNode {
                flags: 1 | if executable { EXECUTABLE } else { 0 },
                children: vec![tail],
                redirect_node: None,
                name: Some(name.to_owned()),
                parser: None,
                suggestions_type: None,
            });
            children.push(index);
        }
        projection.nodes[root as usize].children = children;
        CCommands {
            nodes: projection.nodes,
            root_index: root,
        }
    }

    pub(crate) fn declare_commands_packet(&self, player: &Player) -> PacketEncoder {
        self.declare_commands(|policy| policy.allows(player))
            .encode()
    }
}
