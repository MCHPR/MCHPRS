//! The official plugin api for Minecraft High Performance Redstone Server.
//! 
//! This documentation will guide you through creating your first plugins.

pub use mchprs_plugin_proc_macro::*;

type InitFn = extern "C" fn();

/// Registers the plugin so the server can see it
#[macro_export]
macro_rules! register_plugin {
    ($( $detail_key:ident: $detail_val:expr ),*) => {
        use std::os::raw::c_char;
        use std::ffi::CString;
        struct PluginDetails {
            pub name: &'static str,
            pub version: &'static str
        }
        #[repr(C)]
        struct _PluginDetails {
            name: *const c_char,
            version: *const c_char,
        }
        impl Into<_PluginDetails> for PluginDetails {
            fn into(self) -> _PluginDetails {
                _PluginDetails {
                    name: CString::new(self.name).expect("CString::new failed").as_ptr(),
                    version: CString::new(self.version).expect("CString::new failed").as_ptr(),
                }
            }
        }
        #[no_mangle]
        extern "C" fn _register_plugin() -> *const _PluginDetails {
            let details: PluginDetails = PluginDetails { 
                $( $detail_key: $detail_val, )*
            };
            // Convert into c style struct
            let _details: _PluginDetails = details.into();
            &_details
        }
    };
}
