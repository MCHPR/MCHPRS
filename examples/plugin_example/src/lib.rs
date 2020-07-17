
use mchprs_plugin::{register_plugin, event_handler};

#[event_handler]
fn example_event_handler() {

}

register_plugin!(
    name: "example",
    version: "0.1"
);