#![deny(rust_2018_idioms)]

#[macro_use]
mod utils;
mod chat;
mod config;
mod interaction;
mod permissions;
mod player;
pub mod plot;
mod profile;
pub mod redpiler;
pub mod redstone;
pub mod server;
pub mod world;

#[macro_use]
extern crate bitflags;
