#![deny(rust_2018_idioms)]
#![feature(min_specialization, lazy_cell)]

#[macro_use]
mod utils;
pub mod blocks;
mod chat;
mod config;
mod items;
mod permissions;
mod player;
pub mod plot;
mod profile;
pub mod redpiler;
pub mod server;
pub mod world;

#[macro_use]
extern crate bitflags;
