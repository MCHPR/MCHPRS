use crate::server::MinecraftServer;
use libloading::{Library, Symbol};
use log::info;
use mchprs_plugin::{CPluginDetails, PluginDetails};
use mchprs_plugin::event::{ServerEventHandlerType, ChatEventHandler, ChatEvent};
use std::ffi::CStr;
use std::fs;
use std::os::raw::c_char;
use std::mem;

#[repr(C)]
struct ServerEventContext {
    server: *mut MinecraftServer,
    broadcast_raw_chat: extern fn(ctx: *mut ServerEventContext, raw_message: *mut c_char),
}

impl ServerEventContext {
    fn new(server: &mut MinecraftServer) -> ServerEventContext {
        ServerEventContext {
            server,
            broadcast_raw_chat,
        }
    }
}

extern "C" fn broadcast_raw_chat(ctx: *mut ServerEventContext, raw_message: *mut c_char) {
    let server: &mut MinecraftServer = unsafe { (*ctx).server.as_mut().unwrap() };
    let message = unsafe { CStr::from_ptr(raw_message).to_str().unwrap().to_owned() };
    server.broadcast_raw_chat(message);
}

#[derive(Default)]
struct ServerEventManager {
    chat_event_handlers: Vec<ChatEventHandler>,
}

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
            plugin_manager.libs.push(lib);
            let lib = plugin_manager.libs.last().unwrap();
            let register_func: Symbol<RegisterFunc> =
                unsafe { lib.get(b"_register_plugin").unwrap() };
            let plugin_details: PluginDetails = unsafe { register_func() }.into();
            info!(
                "Loading plugin {}, version {}",
                plugin_details.name, plugin_details.version
            );
            for event_handler in plugin_details.event_handlers {
                match *event_handler {
                    ServerEventHandlerType::ChatEvent(handler) => {
                        plugin_manager.event_manager.chat_event_handlers.push(handler);
                    }
                }
            }
        }
        plugin_manager
    }

    pub fn trigger_chat_event(server: &mut MinecraftServer) {
        let handlers = server.plugin_manager.event_manager.chat_event_handlers.clone();
        if handlers.is_empty() {
            return;
        }
        let context: *mut ServerEventContext = &mut ServerEventContext::new(server);
        for handler in handlers {
            // I don't like this one bit
            unsafe { handler(mem::transmute::<*mut ServerEventContext, mchprs_plugin::event::ServerEventContext>(context), ChatEvent {}) };
        }
    }
}

struct PlotPluginManager {}
