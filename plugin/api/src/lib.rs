//! The official plugin api for Minecraft High Performance Redstone Server.
//!
//! This documentation will guide you through creating your first plugin.

pub mod event;

use event::ServerEventHandlerType;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::slice;

#[repr(C)]
pub struct CPluginDetails {
    pub name: *const c_char,
    pub version: *const c_char,
    pub event_handlers_len: c_int,
    pub event_handlers: *const ServerEventHandlerType,
}

pub struct PluginDetails {
    pub name: &'static str,
    pub version: &'static str,
    pub event_handlers: &'static [ServerEventHandlerType],
}

impl Into<CPluginDetails> for PluginDetails {
    fn into(self) -> CPluginDetails {
        CPluginDetails {
            name: CString::new(self.name)
                .expect("CString::new failed")
                .into_raw(),
            version: CString::new(self.version)
                .expect("CString::new failed")
                .into_raw(),
            event_handlers_len: self.event_handlers.len() as i32,
            event_handlers: self.event_handlers.as_ptr(),
        }
    }
}

impl From<CPluginDetails> for PluginDetails {
    fn from(c: CPluginDetails) -> Self {
        Self {
            name: unsafe { CStr::from_ptr(c.name).to_str().unwrap() },
            version: unsafe { CStr::from_ptr(c.version).to_str().unwrap() },
            event_handlers: unsafe {
                slice::from_raw_parts(c.event_handlers, c.event_handlers_len as usize)
            },
        }
    }
}

/// Registers the plugin so the server can see it
#[macro_export]
macro_rules! register_plugin {
    ($( $detail_key:ident: $detail_val:expr ),*) => {
        use mchprs_plugin::{CPluginDetails, PluginDetails};
        #[no_mangle]
        extern "C" fn _register_plugin() -> CPluginDetails {
            use mchprs_plugin::event::ServerEventHandlerType::*;
            let details: PluginDetails = PluginDetails {
                $( $detail_key: $detail_val, )*
            };
            // Convert into c style struct
            let _details: CPluginDetails = details.into();
            _details
        }
    };
}
