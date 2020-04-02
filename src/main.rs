mod network;
mod player;
mod plot;
mod server;

use server::MinecraftServer;


fn main() {
    MinecraftServer::run();
}
