use std::os::raw::c_char;

pub type InitFn = extern "C" fn();

#[repr(C)]
pub struct CPluginDetails {
    pub name: *const c_char,
    pub version: *const c_char,
    pub init_fn: InitFn,
}