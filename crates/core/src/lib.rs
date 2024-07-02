#![deny(rust_2018_idioms)]

#[macro_use]
mod utils;
mod config;
mod interaction;
mod permissions;
mod player;
pub mod plot;
mod profile;
pub mod server;

#[macro_use]
extern crate bitflags;
