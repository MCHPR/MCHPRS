
use mchprs_plugin::register_plugin;

extern "C" fn init() {

}

register_plugin!(
    name: "example",
    version: "0.1",
    init_fn: init
);