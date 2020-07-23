
use mchprs_plugin::register_plugin;
use mchprs_plugin::event::*;

#[event_handler]
fn handle_chat_event(mut ctx: ServerEventContext, event: ChatEvent) {
    ctx.broadcast_raw_chat(r#"{ "text": "Someone somewhere sent a chat message!" }"#);
}

register_plugin!(
    name: "example",
    version: "0.1",
    event_handlers: &[
        ChatEvent(handle_chat_event),
    ]
);