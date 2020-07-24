//! This plugin will completely take control of chat.

use mchprs_plugin::event::*;
use mchprs_plugin::register_plugin;

#[event_handler]
fn handle_chat_event(mut ctx: ServerEventContext, event: &mut ChatEvent) {
    ctx.broadcast_chat(&format!("<{}> {}", &event.sender_username(), &event.message()));
    event.cancelled = true;
}

register_plugin!(
    name: "example",
    version: "0.1",
    event_handlers: &[
        ChatEvent(handle_chat_event),
    ]
);
