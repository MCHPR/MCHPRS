# Minecraft High Performance Redstone Server

A minecraft creative server built for redstone. Each 128x128 plot runs on its own thread, allowing for less lag and better concurrency.

## Installation

As there are currently no releases, you must build from source.

If the Rust compiler is not already installed, you can find out how [on their official website](https://www.rust-lang.org/tools/install).

```shell
git clone https://github.com/StackDoubleFlow/MCHPRS.git
cd rjvm
cargo build --release
```

Once complete, the optimized executable will be located at `./target/release/mchprs` or `./target/release/mchprs.exe` depending on your operating system.


## Under the hood

When the server starts up, the main thread, which will from now on be referred to as the server thread, loads the `Config.toml` file and starts the networking thread. Threads communicate using message passing.

### Server thread

The server thread handles the initialization process of the server and the login/ping procedure for connecting clients. If a client completes the login procedure, a `Player` struct will be loaded containing the client.

### Networking thread

The networking thread handles all incoming clients. When there is a new client, a client thread is created. The client is then sent to the server thread through message passing.

### Client thread

The client thread manages the TCP connection between the Minecraft client and this server. The client thread sends incoming packets to the server thread or a plot thread depending on its state.

### Plot thread

The plot thread handles most of the logic for the server. This is where the real magic happens. Player movment, player rotation, worldedit, command handling, world saving/loading, etc. is all handled by this thread. If this thread crashes somehow, the player will be sent back to the server thread to be moved to another plot*.