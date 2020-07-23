mod network;
#[macro_use]
mod blocks;
mod items;
mod player;
mod plot;
mod server;
mod plugin;

use server::MinecraftServer;

fn main() {
    MinecraftServer::run();
}
