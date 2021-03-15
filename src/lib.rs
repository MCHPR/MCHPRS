#![feature(const_generics)]

mod network;
#[macro_use]
mod blocks;
mod chat;
mod config;
mod items;
mod player;
mod plot;
#[macro_use]
mod utils;
pub mod server;
pub mod world;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate lazy_static;
