use super::{argument::ArgumentType, node::CommandNode, node::NodeType};
use crate::commands::argument::FlagSpec;
use indexmap::IndexSet;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};

pub fn generate_usage(path: &[&CommandNode]) -> String {
    let mut parts = Vec::new();

    for node in &path[1..] {
        parts.push(get_display_name(node));
    }

    let current_node = path.last().unwrap();

    let suffix = build_usage_suffix(current_node);
    if !suffix.is_empty() {
        parts.push(suffix);
    }

    format!("/{}", parts.join(" "))
}

#[derive(Debug)]
struct UsageStructure {
    content: String,
    trailing_optionals: Vec<String>,
}

fn build_usage_suffix(node: &CommandNode) -> String {
    let structure = analyze_structure(node);

    let mut parts = Vec::new();
    if !structure.content.is_empty() {
        parts.push(structure.content);
    }

    for opt in structure.trailing_optionals {
        parts.push(format!("[{}]", opt));
    }

    parts.join(" ")
}

fn analyze_structure(node: &CommandNode) -> UsageStructure {
    if node.children.is_empty() {
        return UsageStructure {
            content: String::new(),
            trailing_optionals: Vec::new(),
        };
    }

    let (optional_children, regular_children): (Vec<_>, Vec<_>) =
        node.children.iter().partition(|child| is_greedy(child));

    let mut all_optionals = HashSet::new();
    for child in &optional_children {
        all_optionals.insert(get_display_name(child));
    }

    let children_optional = node.has_executor() || !optional_children.is_empty();

    let content = if regular_children.is_empty() {
        String::new()
    } else {
        format_regular_children(&regular_children, children_optional, &mut all_optionals)
    };

    let trailing_optionals = all_optionals
        .into_iter()
        .map(|name| name.to_string())
        .sorted()
        .collect_vec();

    UsageStructure {
        content,
        trailing_optionals,
    }
}

fn format_regular_children(
    children: &[&CommandNode],
    make_optional: bool,
    all_optionals: &mut HashSet<String>,
) -> String {
    if children.is_empty() {
        return String::new();
    }

    let mut groups: HashMap<String, Vec<&CommandNode>> = HashMap::new();
    for child in children {
        let key = get_display_name(child);
        groups.entry(key).or_default().push(child);
    }

    if children.len() == 1 {
        let child = children[0];
        let child_structure = analyze_structure(child);

        for opt in child_structure.trailing_optionals {
            all_optionals.insert(opt);
        }

        let child_name = get_display_name(child);
        let result = if child_structure.content.is_empty() {
            child_name
        } else {
            format!("{} {}", child_name, child_structure.content)
        };

        if make_optional {
            format!("[{}]", result)
        } else {
            result
        }
    } else if groups.len() == 1 && children.len() > 1 {
        let child_names: Vec<String> = children.iter().map(|c| get_display_name(c)).collect();

        let alternatives = child_names.join(" | ");
        if make_optional {
            format!("[{}]", alternatives)
        } else {
            format!("({})", alternatives)
        }
    } else {
        let mut child_parts = Vec::new();

        for child in children {
            let child_structure = analyze_structure(child);

            for opt in child_structure.trailing_optionals {
                all_optionals.insert(opt);
            }

            let child_name = get_display_name(child);
            let part = if child_structure.content.is_empty() {
                child_name
            } else {
                format!("{} {}", child_name, child_structure.content)
            };

            child_parts.push(part);
        }

        let alternatives = child_parts.join(" | ");
        if make_optional {
            format!("[{}]", alternatives)
        } else {
            format!("({})", alternatives)
        }
    }
}

fn is_greedy(node: &CommandNode) -> bool {
    matches!(&node.node_type, NodeType::Argument { arg_type, .. }
            if matches!(arg_type, ArgumentType::GreedyString | ArgumentType::Flags { .. }))
}

fn get_display_name(node: &CommandNode) -> String {
    match &node.node_type {
        NodeType::Root => unreachable!(),
        NodeType::Literal { name, .. } => name.clone(),
        NodeType::Argument { name, .. } => format!("<{}>", name),
    }
}

pub fn generate_flag_details(node: &CommandNode) -> Vec<String> {
    let mut flags = IndexSet::default();
    get_flag_details_from_node(&mut flags, node);
    flags
        .into_iter()
        .map(|spec| match spec.short {
            Some(short) => format!("-{} | --{}", short, spec.long),
            None => format!("--{}", spec.long),
        })
        .collect()
}

fn get_flag_details_from_node(all_flags: &mut IndexSet<FlagSpec>, node: &CommandNode) {
    if let NodeType::Argument {
        arg_type: ArgumentType::Flags { flags },
        ..
    } = &node.node_type
    {
        all_flags.extend(flags.iter().cloned());
    }
    for child in &node.children {
        get_flag_details_from_node(all_flags, child);
    }
}

pub fn generate_base_name(path: &[&CommandNode]) -> String {
    let mut parts = Vec::new();

    for node in path {
        match &node.node_type {
            NodeType::Root => {}
            NodeType::Literal { name, .. } => parts.push(name),
            NodeType::Argument { .. } => break,
        }
    }

    format!("/{}", parts.into_iter().join(" "))
}
