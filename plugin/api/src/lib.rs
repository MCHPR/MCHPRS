//! The official plugin api for Minecraft High Performance Redstone Server.
//! 
//! This documentation will guide you through creating your first plugins.

pub use mchprs_plugin_proc_macro::*;
use std::ffi::CString;
use std::os::raw::c_char;

pub type InitFn = extern "C" fn();

#[repr(C)]
pub struct CPluginDetails {
    pub name: *const c_char,
    pub version: *const c_char,
    pub init_fn: InitFn,
}

pub struct PluginDetails {
    name: &'static str,
    version: &'static str,
    init_fn: InitFn,
}

impl Into<CPluginDetails> for PluginDetails {
    fn into(self) -> CPluginDetails {
        CPluginDetails {
            name: CString::new(self.name).expect("CString::new failed").into_raw(),
            version: CString::new(self.version).expect("CString::new failed").into_raw(),
            init_fn: self.init_fn,
        }
    }
}

/// Registers the plugin so the server can see it
#[macro_export]
macro_rules! register_plugin {
    ($( $detail_key:ident: $detail_val:expr ),*) => {
        #[no_mangle]
        extern "C" fn _register_plugin() -> *const CPluginDetails {
            let details: PluginDetails = PluginDetails { 
                $( $detail_key: $detail_val, )*
            };
            // Convert into c style struct
            let _details: CPluginDetails = details.into();
            &_details
        }
    };
}
