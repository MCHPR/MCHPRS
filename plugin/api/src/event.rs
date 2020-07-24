pub use mchprs_plugin_proc_macro::event_handler;
use std::ffi::{c_void, CString, CStr};
use std::os::raw::{c_char, c_uchar};

#[repr(C)]
struct CServerEventContext {
    server: *mut c_void,
    broadcast_raw_chat: extern "C" fn(*mut CServerEventContext, *mut c_char),
    broadcast_chat: extern "C" fn(*mut CServerEventContext, *mut c_char),
}

impl CServerEventContext {
    fn broadcast_raw_chat(ctx: *mut CServerEventContext, message: &str) {
        unsafe { ((*ctx).broadcast_raw_chat)(ctx, CString::new(message).unwrap().into_raw()) }
    }


    fn broadcast_chat(ctx: *mut CServerEventContext, message: &str) {
        unsafe { ((*ctx).broadcast_chat)(ctx, CString::new(message).unwrap().into_raw()) }
    }
}

#[repr(transparent)]
pub struct ServerEventContext(*mut CServerEventContext);

impl ServerEventContext {
    pub fn broadcast_raw_chat(&mut self, message: &str) {
        CServerEventContext::broadcast_raw_chat(self.0, message);
    }
    pub fn broadcast_chat(&mut self, message: &str) {
        CServerEventContext::broadcast_chat(self.0, message);
    }
}

#[repr(C)]
pub enum ServerEventHandlerType {
    ChatEvent(ChatEventHandler),
}

#[repr(C)]
pub struct ChatEvent {
    pub cancelled: bool,
    sender_uuid: [c_uchar; 16],
    sender_username: *const c_char,
    message: *const c_char,
}

impl ChatEvent {
    pub fn new(sender_uuid: [c_uchar; 16], sender_username: *const c_char, message: *const c_char) -> ChatEvent {
        ChatEvent {
            cancelled: false,
            message,
            sender_username,
            sender_uuid,
        }
    }

    pub fn sender_uuid(&self) -> u128 {
        u128::from_be_bytes(self.sender_uuid)
    }

    pub fn sender_username(&self) -> &str {
        unsafe { CStr::from_ptr(self.sender_username).to_str().unwrap() }
    }

    pub fn message(&self) -> &str {
        unsafe { CStr::from_ptr(self.message).to_str().unwrap() }
    }
}

pub type ChatEventHandler = extern "C" fn(ServerEventContext, &mut ChatEvent);
