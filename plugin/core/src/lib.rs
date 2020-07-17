pub use mchprs_plugin_proc_macro::*;

#[repr(C)]
pub struct PluginDetails {}

type RegisterFn = extern "C" fn

#[macro_export]
macro_rules! register_plugin {
    () => {
        #[no_mangle]
        extern "C" fn _register_plugin() -> *$crate::PluginDetails {

        }
    };
}
