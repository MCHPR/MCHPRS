
use mchprs_plugin::{register_plugin, event_handler};

#[event_handler]
fn example_event_handler() {

}

extern "C" fn init() {

}

register_plugin!(
    name: "example",
    version: "0.1",
    init_fn: init,
);