use crate::commands::{
    argument::ArgumentType,
    argument_parser,
    node::{CommandNode, NodeType},
    value::Value,
};

pub enum ParseResult<'a> {
    Success {
        node: &'a CommandNode,
        arguments: Vec<(String, Value)>,
        path: Vec<&'a CommandNode>,
    },
    Partial {
        node: &'a CommandNode,
        arguments: Vec<(String, Value)>,
        path: Vec<&'a CommandNode>,
    },
    TooManyArguments {
        node: &'a CommandNode,
        arguments: Vec<(String, Value)>,
        path: Vec<&'a CommandNode>,
        remaining: String,
    },
    InvalidArgument {
        node: &'a CommandNode,
        arguments: Vec<(String, Value)>,
        path: Vec<&'a CommandNode>,
        remaining: String,
    },
    NothingMatched {
        root: &'a CommandNode,
    },
}

pub(super) fn parse<'a, 'b>(root: &'a CommandNode, input: &'b str) -> ParseResult<'a> {
    let mut remaining_input = input;
    let mut path: Vec<&'a CommandNode> = vec![root];
    let mut arguments: Vec<(String, Value)> = Vec::new();

    while parse_next(&mut path, &mut arguments, &mut remaining_input) {}

    let final_node = path.last().unwrap();

    if matches!(final_node.node_type, NodeType::Root) {
        return ParseResult::NothingMatched { root: final_node };
    }

    let remainder = remaining_input.trim_start();

    if !remainder.is_empty() {
        if final_node.has_executor() && final_node.children.is_empty() {
            ParseResult::TooManyArguments {
                node: final_node,
                arguments,
                path,
                remaining: remainder.to_string(),
            }
        } else {
            ParseResult::InvalidArgument {
                node: final_node,
                arguments,
                path,
                remaining: remainder.to_string(),
            }
        }
    } else if final_node.has_executor() {
        ParseResult::Success {
            node: final_node,
            arguments,
            path,
        }
    } else {
        ParseResult::Partial {
            node: final_node,
            arguments,
            path,
        }
    }
}

fn parse_next(
    path: &mut Vec<&CommandNode>,
    arguments: &mut Vec<(String, Value)>,
    remaining_input: &mut &str,
) -> bool {
    let current_node = path.last().unwrap();

    for child in &current_node.children {
        match &child.node_type {
            NodeType::Root => continue,

            NodeType::Literal { name, aliases } => {
                let input = remaining_input.trim_start();
                let Some((token, _)) = argument_parser::consume_token(input) else {
                    continue;
                };

                if token == name || aliases.iter().any(|alias| alias == token) {
                    let (_, rest) = argument_parser::consume_token(input).unwrap();
                    path.push(child);
                    *remaining_input = rest;
                    return true;
                }
            }

            NodeType::Argument { name, arg_type } => {
                if let Ok((value, rest)) = arg_type.parse(remaining_input) {
                    path.push(child);
                    arguments.push((name.clone(), value));
                    *remaining_input = rest;
                    return true;
                }
            }
        }
    }

    false
}

pub fn backtrack_overgreedy_matches<'a>(
    mut path: Vec<&'a CommandNode>,
    arguments: &[(String, Value)],
) -> Vec<&'a CommandNode> {
    while let Some(last_node) = path.last() {
        if let NodeType::Argument { name, arg_type } = &last_node.node_type {
            let is_greedy = matches!(
                arg_type,
                ArgumentType::Flags { .. } | ArgumentType::GreedyString
            );

            if is_greedy {
                if let Some((_, value)) = arguments.iter().find(|(arg_name, _)| arg_name == name) {
                    let is_empty = match value {
                        Value::Flags(set) => set.is_empty(),
                        Value::GreedyString(s) => s.is_empty(),
                        _ => false,
                    };

                    if is_empty {
                        path.pop();
                        continue;
                    }
                }
            }
        }

        break;
    }

    path
}
