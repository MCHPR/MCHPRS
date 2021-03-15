#![deny(rust_2018_idioms)]

#[macro_use]
mod utils;
#[macro_use]
mod blocks;
mod chat;
mod config;
mod items;
mod network;
mod player;
mod plot;
pub mod server;
pub mod world;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate lazy_static;
