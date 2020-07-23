use crate::server::MinecraftServer;
use libloading::{os::unix::Symbol as RawSymbol, Library, Symbol};
use log::info;
use mchprs_plugin::{CPluginDetails, PluginDetails};
use std::ffi::CStr;
use std::fs;
use std::os::raw::c_char;

unsafe fn broadcast_raw_chat(ctx: *mut CServerEventContext, raw_message: *mut c_char) {
    let server: &mut MinecraftServer = unsafe { (*ctx).server.as_mut().unwrap() };
    let message = CStr::from_ptr(raw_message).to_str().unwrap().to_owned();
    server.broadcast_raw_chat(message);
}

#[repr(C)]
struct CServerEventContext {
    server: *mut MinecraftServer,
    broadcast_raw_chat: fn(),
}

#[derive(Default)]
struct ServerEventManager {}

pub struct ServerPluginManager {
    libs: Vec<Library>,
    event_manager: ServerEventManager,
}

type RegisterFunc = unsafe extern "C" fn() -> CPluginDetails;

impl ServerPluginManager {
    pub fn load_plugins() -> ServerPluginManager {
        let mut plugin_manager = ServerPluginManager {
            libs: Vec::new(),
            event_manager: Default::default(),
        };
        fs::create_dir_all("./plugins").unwrap();
        let dir = fs::read_dir("./plugins").unwrap();
        for entry in dir {
            let entry = entry.unwrap();
            let lib = Library::new(entry.path()).unwrap();
            let register_func: Symbol<RegisterFunc> =
                unsafe { lib.get(b"_register_plugin").unwrap() };
            let plugin_details: PluginDetails = unsafe { register_func() }.into();
            info!(
                "Loading plugin {}, version {}",
                plugin_details.name, plugin_details.version
            );
        }
        plugin_manager
    }
}

struct PlotPluginManager {}
