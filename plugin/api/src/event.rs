
pub use mchprs_plugin_proc_macro::event_handler;
use std::ffi::{c_void, CString};
use std::os::raw::c_char;

#[repr(C)]
struct CServerEventContext {
    server: *mut c_void,
    broadcast_raw_chat: extern fn(*mut CServerEventContext, *mut c_char),
}

impl CServerEventContext {
    fn broadcast_raw_chat(ctx: *mut CServerEventContext, message: &str) {
        unsafe { ((*ctx).broadcast_raw_chat)(ctx, CString::new(message).unwrap().into_raw()) }
    }
}

#[repr(transparent)]
pub struct ServerEventContext(*mut CServerEventContext);

impl ServerEventContext {
    pub fn broadcast_raw_chat(&mut self, message: &str) { 
        CServerEventContext::broadcast_raw_chat(self.0, message);
    }
}



#[repr(C)]
pub enum ServerEventHandlerType {
    ChatEvent(ChatEventHandler),
}

pub struct ChatEvent {

}

pub type ChatEventHandler = extern fn(ServerEventContext, ChatEvent);