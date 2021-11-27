use super::node::{Node, NodeType};
use std::collections::HashMap;

pub fn constant_fold(nodes: &mut HashMap<usize, Node>) {
    let mut foldable = 0;
    let mut set_constant = Vec::new();
    loop {
        let old_foldable = foldable;
        'nodes: for node_id in nodes.keys() {
            let node = &nodes[node_id];
            if matches!(node.ty, NodeType::Constant | NodeType::Lever | NodeType::StonePressurePlate | NodeType::StoneButton)  {
                continue;
            }
            for input in &node.inputs {
                if nodes[&input.end].ty != NodeType::Constant {
                    continue 'nodes;
                }
            }
            foldable += 1;
            set_constant.push(*node_id);
        }
        for id in set_constant.drain(..) {
            nodes.get_mut(&id).unwrap().ty = NodeType::Constant;
        }
        if foldable == old_foldable {
            break;
        }
    }
}

pub fn remove_redundant(nodes: &mut HashMap<usize, Node>) {
    let mut to_remove = Vec::new();
    for (id, node) in nodes.iter() {
        // Can't remove lamps
        if node.ty == NodeType::Lamp {
            continue;
        }

        // These nodes have a purpose
        if node.updates.is_empty() || node.updates.iter().any(|out| nodes.contains_key(out) && nodes[out].ty != NodeType::Constant) {
            continue;
        }

        to_remove.push(*id);
    }
    println!("Removing {} redundant nodes", to_remove.len());
    for id in to_remove {
        nodes.remove(&id);
    }
}