# Minecraft High-Performance Redstone Server

[![Build Status](https://travis-ci.org/MCHPR/MCHPRS.svg?branch=master)](https://travis-ci.org/MCHPR/MCHPRS) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![Crates.io](https://img.shields.io/crates/v/mchprs?colorB=319e8c)](https://crates.io/crates/mchprs)
[![Discord Banner 2](https://discordapp.com/api/guilds/724072903083163679/widget.png)](https://discord.com/invite/svK9JU7)


A Minecraft creative server built for redstone. Each 256x256 plot runs on a separate thread, allowing for less lag, more concurrency, and many awesome extra features!

MCHPRS is very different from traditional servers. Because this server is tailored to the use of computation redstone, many things that are a part of Vanilla Minecraft servers don't exist here. That being said, MCHPRS comes with many of its own unique features.

## Building

If the Rust compiler is not already installed, you can find out how [on their official website](https://www.rust-lang.org/tools/install).

```shell
git clone https://github.com/MCHPR/MCHPRS.git
cd MCHPRS
cargo build --release
```

Once complete, the optimized executable will be located at `./target/release/mchprs` or `./target/release/mchprs.exe` depending on your operating system.

### Building on Windows

To build on Windows, replace the last line of `Cargo.toml`  with
```toml 
rusqlite = {version="0.24.0", features=["bundled"]}
```

## Usage

### Commands
| Command | Alias | Description |
| --- | --- |--- |
| `/rtps [rtps]` | None | Set the **redstone** ticks per second in the plot to `[rtps]`. (There are two redstone ticks in a game tick) |
| `/radvance [ticks]` | `/radv` | Advances the plot by `[ticks]` redstone ticks. |
| `/teleport [player]` | `/tp` | Teleports you to `[player]`. |
| `/stop` | None | Stops the server. |
| `/plot info` | `/p i` | Gets the owner of the plot you are in. |
| `/plot claim` | `/p c` | Claims the plot you are in if it is not already claimed. |
| `//pos1` | `//1` | Sets your worldedit first position. |
| `//pos2` | `//2` | Sets your worldedit second position. |
| `//set [block]` | None | Sets all the blocks in your selection to `[block]` |
| `//replace [oldblock] [newblock]` | None | Replaces all of the `[oldblock]` in your selection with `[newblock]`. |
| `//copy` | `//c` | Copies your selection into your clipboard. |
| `//paste` | `//p` | Pastes your clipboard into the world. |
| `//undo` | None | Undos the last operation. |
| `//sel` | None | Clears your worldedit first and second positions. |
| `//stack` | None | Stacks your selection in the direction you are facing. |
| `//count [block]` | None | Counts all `[block]` in your selection. |
| `//load` | None | Loads a schematic from the `./schems/` folder. Make sure the schematic in the Sponge format if there are any issues. |

## Acknowledgments
- [@AL1L](https://github.com/AL1L) for his contributions to worldedit and other various features.
- [@DavidGarland](https://github.com/DavidGarland) for a faster and overall better implementation of `get_entry` in the in-memory storage. This simple function runs 30% of the runtime for redstone.

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## Under the hood

When the server starts up, the main thread, which will from now on be referred to as the server thread, loads the `Config.toml` file and starts the networking thread. Threads communicate using message passing.

### Server thread

The server thread handles the initialization process of the server and the login/ping procedure for connecting clients. If a client completes the login procedure, a `Player` struct will be loaded containing the client.

### Networking thread

The networking thread handles all incoming clients. The client is then sent to the server thread through message passing.

### Plot thread

The plot thread handles most of the logic for the server. The plot thread is where the real magic happens. Player movement, player rotation, WorldEdit, command handling, world-saving/loading, etc. are all handled by this thread. If this thread crashes somehow, the player will be sent back to the server thread to be moved to another plot*.

\* Not yet implemented.

## License
[MIT](https://choosealicense.com/licenses/mit/)
