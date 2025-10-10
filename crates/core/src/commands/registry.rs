use super::{autocomplete, node::CommandNode};
use mchprs_network::packets::PacketEncoder;

pub struct CommandRegistry {
    root: CommandNode,
    declare_commands_packet: Option<PacketEncoder>,
    custom_aliases: Vec<(String, String)>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            root: CommandNode::root(),
            declare_commands_packet: None,
            custom_aliases: Vec::new(),
        }
    }

    pub fn register(&mut self, command: CommandNode) {
        self.root.children.push(command);
    }

    pub fn get_root(&self) -> &CommandNode {
        &self.root
    }

    pub fn rebuild_declare_commands_packet(&mut self) {
        self.declare_commands_packet = Some(autocomplete::generate_declare_commands_packet(self));
    }

    pub fn get_declare_commands_packet(&self) -> &PacketEncoder {
        self.declare_commands_packet.as_ref().unwrap()
    }

    pub fn add_custom_alias(&mut self, prefix: impl Into<String>, replacement: impl Into<String>) {
        self.custom_aliases
            .push((prefix.into(), replacement.into()));
    }

    pub fn get_custom_aliases(&self) -> &[(String, String)] {
        &self.custom_aliases
    }
}
