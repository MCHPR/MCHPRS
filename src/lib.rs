#![deny(rust_2018_idioms)]
#![feature(min_specialization, once_cell)]

#[macro_use]
mod utils;
mod blocks;
mod chat;
mod config;
mod items;
mod network;
mod permissions;
mod player;
mod plot;
mod profile;
mod redpiler;
pub mod server;
pub mod world;

#[macro_use]
extern crate bitflags;
