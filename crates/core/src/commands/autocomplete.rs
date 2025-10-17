use crate::commands::{
    argument::ArgumentType,
    node::{CommandNode, NodeType},
    parser,
    registry::CommandRegistry,
};
use itertools::Itertools;
use mchprs_network::packets::{
    clientbound::{CCommands, CCommandsNode, CDeclareCommandsNodeParser, ClientBoundPacket},
    PacketEncoder,
};
use rustc_hash::FxHashMap;
use tracing::{debug, warn};

type NodeId = i32;

bitflags! {
    #[derive(Copy, Clone)]
    struct CommandFlags: u32 {
        const ROOT = 0x0;
        const LITERAL = 0x1;
        const ARGUMENT = 0x2;
        const EXECUTABLE = 0x4;
        const REDIRECT = 0x8;
        const HAS_SUGGESTIONS_TYPE = 0x10;
    }
}

pub(super) fn generate_declare_commands_packet(registry: &CommandRegistry) -> PacketEncoder {
    let mut nodes = Vec::new();
    let mut node_map = FxHashMap::default();

    let root = registry.get_root();
    let root_node_ids = build_node_tree(&mut nodes, root, &mut node_map);
    let root_node_id = root_node_ids.into_iter().exactly_one().unwrap();

    let custom_aliases = registry.get_custom_aliases();
    for (alias, replacement) in custom_aliases {
        build_custom_alias_node(
            &mut nodes,
            root,
            root_node_id,
            &node_map,
            alias,
            replacement,
        );
    }

    let packet = CCommands {
        nodes,
        root_index: root_node_id,
    };

    packet.encode()
}

fn build_node_tree(
    nodes: &mut Vec<CCommandsNode>,
    node: &CommandNode,
    node_map: &mut FxHashMap<*const CommandNode, Vec<NodeId>>,
) -> Vec<NodeId> {
    let mut children = Vec::new();
    for child in &node.children {
        let child_ids = build_node_tree(nodes, child, node_map);
        children.extend(&child_ids);
    }

    let mut flags = CommandFlags::empty();

    let has_greedy_child = node.children.iter().any(|child| {
        matches!(&child.node_type, NodeType::Argument { arg_type, .. }
            if matches!(arg_type, ArgumentType::GreedyString | ArgumentType::Flags { .. }))
    });

    if node.has_executor() || has_greedy_child {
        flags |= CommandFlags::EXECUTABLE;
    }

    let result = match &node.node_type {
        NodeType::Root => {
            vec![build_node(
                nodes,
                CCommandsNode {
                    flags: CommandFlags::ROOT.bits() as i8,
                    children,
                    redirect_node: None,
                    name: None,
                    parser: None,
                    suggestions_type: None,
                },
            )]
        }
        NodeType::Literal { name, aliases } => {
            let idx = build_literal_node(nodes, name.clone(), flags, children);
            let alias_nodes = aliases
                .iter()
                .map(|alias| build_alias_node(nodes, alias, idx));
            [idx].into_iter().chain(alias_nodes).collect()
        }
        NodeType::Argument { name, arg_type } => {
            build_autocomplete_nodes(nodes, name.clone(), arg_type, flags, children)
        }
    };

    node_map.insert(node as *const CommandNode, result.clone());
    result
}

fn build_custom_alias_node(
    nodes: &mut Vec<CCommandsNode>,
    root: &CommandNode,
    root_node_id: NodeId,
    node_map: &FxHashMap<*const CommandNode, Vec<NodeId>>,
    alias: &str,
    replacement: &str,
) {
    // TODO: Parse only up to {} placeholder for autocomplete
    // In reality we would need to create an entirely separate tree, but this works for most cases
    let replacement_for_parse = if let Some(pos) = replacement.find("{}") {
        replacement[..pos].trim()
    } else {
        replacement
    };

    let Some(target) = find_target_node_id(root, replacement_for_parse) else {
        return;
    };

    let node_ids = node_map
        .get(&(target as *const CommandNode))
        .cloned()
        .unwrap_or_default();

    for target_id in node_ids {
        let alias_id = build_alias_node(nodes, alias, target_id);
        nodes[root_node_id as usize].children.push(alias_id);
    }
}

fn find_target_node_id<'a>(root: &'a CommandNode, replacement: &str) -> Option<&'a CommandNode> {
    let parse_result = parser::parse(root, replacement);

    match parse_result {
        parser::ParseResult::Success { node, path, .. } => {
            debug!(
                "Custom alias replacement '{}' successfully parsed",
                replacement
            );
            let is_greedy = matches!(&node.node_type, NodeType::Argument { arg_type, .. }
                if matches!(arg_type, ArgumentType::GreedyString | ArgumentType::Flags { .. }));
            if is_greedy {
                path.get(path.len() - 2).copied()
            } else {
                Some(node)
            }
        }
        parser::ParseResult::Partial { node, .. } => {
            debug!("Custom alias replacement '{}' is incomplete", replacement);
            Some(node)
        }
        parser::ParseResult::TooManyArguments {
            node, remaining, ..
        } => {
            debug!(
                "Custom alias replacement '{}' has too many arguments: '{}'",
                replacement, remaining
            );
            Some(node)
        }
        parser::ParseResult::InvalidArgument {
            node, remaining, ..
        } => {
            debug!(
                "Custom alias replacement '{}' has invalid argument: '{}'",
                replacement, remaining
            );
            Some(node)
        }
        parser::ParseResult::NothingMatched { .. } => {
            warn!(
                "Custom alias replacement '{}' did not match any command",
                replacement
            );
            None
        }
    }
}

fn build_autocomplete_nodes(
    nodes: &mut Vec<CCommandsNode>,
    name: String,
    arg_type: &ArgumentType,
    flags: CommandFlags,
    children: Vec<NodeId>,
) -> Vec<NodeId> {
    match arg_type {
        ArgumentType::String => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::String(0),
                flags,
                children,
            )]
        }
        ArgumentType::Integer { min, max } => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::Integer(*min, *max),
                flags,
                children,
            )]
        }
        ArgumentType::Float { min, max } => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::Float(*min, *max),
                flags,
                children,
            )]
        }
        ArgumentType::Boolean => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::Bool,
                flags,
                children,
            )]
        }
        ArgumentType::Player => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::Entity(3),
                flags,
                children,
            )]
        }
        ArgumentType::Direction => [
            "up", "down", "north", "south", "east", "west", "me", "left", "right",
        ]
        .iter()
        .map(|&literal_name| {
            build_literal_node(nodes, literal_name.to_string(), flags, children.clone())
        })
        .collect(),
        ArgumentType::Vec3 => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::Vec3,
                flags,
                children,
            )]
        }
        ArgumentType::ColumnPos => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::ColumnPos,
                flags,
                children,
            )]
        }
        ArgumentType::Container => ["barrel", "furnace", "hopper"]
            .iter()
            .map(|&literal_name| {
                build_literal_node(nodes, literal_name.to_string(), flags, children.clone())
            })
            .collect(),
        ArgumentType::Pattern => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::String(0),
                flags,
                children,
            )]
        }
        ArgumentType::Mask => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::String(0),
                flags,
                children,
            )]
        }
        ArgumentType::DirectionExt => [
            "up",
            "down",
            "north",
            "south",
            "east",
            "west",
            "northup",
            "northdown",
            "southup",
            "southdown",
            "eastup",
            "eastdown",
            "westup",
            "westdown",
            "me",
            "left",
            "right",
            "leftup",
            "leftdown",
            "rightup",
            "rightdown",
        ]
        .iter()
        .map(|&literal_name| {
            build_literal_node(nodes, literal_name.to_string(), flags, children.clone())
        })
        .collect(),
        ArgumentType::BlockPos => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::BlockPos,
                flags,
                children,
            )]
        }
        ArgumentType::GreedyString => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::String(2),
                flags,
                children,
            )]
        }
        ArgumentType::Flags { .. } => {
            vec![build_arg_node(
                nodes,
                name,
                CDeclareCommandsNodeParser::String(2),
                flags,
                children,
            )]
        }
    }
}

fn build_arg_node(
    nodes: &mut Vec<CCommandsNode>,
    name: String,
    parser: CDeclareCommandsNodeParser,
    flags: CommandFlags,
    children: Vec<NodeId>,
) -> NodeId {
    build_node(
        nodes,
        CCommandsNode {
            flags: (flags | CommandFlags::ARGUMENT).bits() as i8,
            children,
            redirect_node: None,
            name: Some(name),
            parser: Some(parser),
            suggestions_type: None,
        },
    )
}

fn build_literal_node(
    nodes: &mut Vec<CCommandsNode>,
    name: String,
    flags: CommandFlags,
    children: Vec<NodeId>,
) -> NodeId {
    build_node(
        nodes,
        CCommandsNode {
            flags: (flags | CommandFlags::LITERAL).bits() as i8,
            children,
            redirect_node: None,
            name: Some(name),
            parser: None,
            suggestions_type: None,
        },
    )
}

fn build_alias_node(nodes: &mut Vec<CCommandsNode>, alias: &str, target_id: NodeId) -> NodeId {
    build_node(
        nodes,
        CCommandsNode {
            flags: (CommandFlags::LITERAL | CommandFlags::REDIRECT).bits() as i8,
            children: vec![],
            redirect_node: Some(target_id),
            name: Some(alias.to_string()),
            parser: None,
            suggestions_type: None,
        },
    )
}

fn build_node(nodes: &mut Vec<CCommandsNode>, node: CCommandsNode) -> NodeId {
    let node_id = nodes.len() as NodeId;
    nodes.push(node);
    node_id
}
