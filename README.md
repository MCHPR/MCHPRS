# Minecraft High-Performance Redstone Server

[![Build Status](https://github.com/MCHPR/MCHPRS/actions/workflows/build.yml/badge.svg)](https://github.com/MCHPR/MCHPRS/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Discord Banner 2](https://discordapp.com/api/guilds/724072903083163679/widget.png)](https://discord.com/invite/svK9JU7)

A Minecraft 1.20.4 creative server built for redstone. Each 512x512 plot runs on a separate thread, allowing for less lag, more concurrency, and many awesome extra features!

MCHPRS is very different from traditional servers. Because this server is tailored to the use of computation redstone, many things that are a part of Vanilla Minecraft servers don't exist here. That being said, MCHPRS comes with many of its own unique features.

MCHPRS has made it possible to run programs such as [a limited form of Minecraft](https://www.youtube.com/watch?v=-BP7DhHTU-I) on CPUs in Minecraft. To accomplish these speeds, we created [Redpiler](docs/Redpiler.md), the "Redstone Compiler".

## Table of Contents

- [Table of Contents](#table-of-contents)
- [Building](#building)
- [Configuration](#configuration)
    - [LuckPerms](#luckperms)
- [Usage](#usage)
    - [General Commands](#general-commands)
    - [Plot Ownership](#plot-ownership)
    - [Worldedit](#worldedit)
    - [Redpiler](#redpiler)
- [Acknowledgments](#acknowledgments)
- [Contributing](#contributing)
- [License](#license)

## Building

If the Rust compiler is not already installed, you can find out how [on their official website](https://www.rust-lang.org/tools/install).

```shell
git clone https://github.com/MCHPR/MCHPRS.git
cd MCHPRS
cargo build --release
```

Once complete, the optimized executable will be located at `./target/release/mchprs` or `./target/release/mchprs.exe` depending on your operating system.

## Configuration

MCHPRS will generate a `Config.toml` file in the current working directory when starting the server if it does not exist.

The folowing options are available at the toplevel (under no header):
| Field | Description | Default |
| --- | --- |--- |
| `bind_address` | Bind address and port | `0.0.0.0:25565` |
| `motd` | Message of the day | `"Minecraft High Performance Redstone Server"` |
| `chat_format` | How to format chat message interpolating `username` and `message` with curly braces | `<{username}> {message}` |
| `max_players` | Maximum number of simultaneous players | `99999` |
| `view_distance` | Maximal distance (in chunks) between players and loaded chunks | `8` |
| `whitelist` | Whether or not the whitelist (in `whitelist.json`) shoud be enabled | `false` |
| `schemati` | Mimic the verification and directory layout used by the Open Redstone Engineers [Schemati plugin](https://github.com/OpenRedstoneEngineers/Schemati) | `false` |
| `block_in_hitbox` | Allow placing blocks inside of players (hitbox logic is simplified) | `true` |
| `auto_redpiler` | Use redpiler automatically | `false` |

To change the plot size edit the constants defined in [plot/mod.rs](./crates/core/src/plot/mod.rs).

### Velocity

MCHPRS has no support for player authentication on its own, but supports Velocity modern ip-forwarding.

To use [Velocity](https://papermc.io/software/velocity) ip-forwarding, you must have a Velocity proxy set up and configured. Make sure `player-info-forwarding-mode` is set to `modern` in your Velocity config. Then, append this to your `Config.toml`:

```toml
[velocity]
enabled = true
# This is the secret contained within `forwarding-secret-file` from your velocity config,
# NOT the path to the file.
secret = "<secret>"
```

### LuckPerms

MCHPRS has basic support for LuckPerms with MySQL or MariaDB remote database storage. This implementation has no commands or interface and would have to be manged through LuckPerms running on a proxy (`/lpb`) or other server (`/lp`)

To use LuckPerms, append this to your `Config.toml`:

```toml
[luckperms]
# Define the address for the database.
host = "localhost"
# The name of the database the LuckPerms data is in.
db_name = "minecraft"
# Credentials for the database.
username = "minecraft"
password = "minecraft"
# The name of the server, used for server specific permissions.
# See: https://luckperms.net/wiki/Context
server_context = "global"
```

## Usage

### General Commands
| Command | Alias | Description |
| --- | --- |--- |
| `/rtps [rtps\|unlimited]` | None | Set the **redstone** ticks per second in the plot to `[rtps]`. (There are two game ticks in a redstone tick) |
| `/radvance [ticks]` | `/radv` | Advances the plot by `[ticks]` redstone ticks. |
| `/teleport [player]` | `/tp` | Teleports you to `[player]`. |
| `/teleport [x] [y] [z]` | `/tp` | Teleports you to `[x] [y] [z]`. Supports relative coordinates. Floats can be expressed as described [here](https://doc.rust-lang.org/std/primitive.f64.html#grammar). |
| `/speed [speed]` | None | Sets your flyspeed. |
| `/gamemode [mode]` | `/gmc`, `/gmsp` | Sets your gamemode. |
| `/container [type] [power]` | None | Gives you a container (e.g. barrel) which outputs a specified amount of power when used with a comparator. |
| `/worldsendrate <hertz>` | `/wsr` | Sets the world send rate to `<hertz>` (frequency of world updates sent to clients). Range: 1-1000. Default: 60. |
| `/toggleautorp` | None | Toggles automatic redpiler compilation. |
| `/stop` | None | Stops the server. |

### Plot Ownership
The plot ownership system in MCHPRS is very incomplete.
These are the commands that are currently implemented:
| Command | Alias | Description |
| --- | --- |--- |
| `/plot info` | `/p i` | Gets the owner of the plot you are in. |
| `/plot claim` | `/p c` | Claims the plot you are in if it is not already claimed. |
| `/plot auto` | `/p a` | Automatically finds an unclaimed plot and claims. |
| `/plot middle` | None | Teleports you to the center of the plot you are in. |
| `/plot visit [player]` | `/p v` | Teleports you to a player's plot. |
| `/plot tp [x] [z]` | None | Teleports you to the plot at `[x] [y]`. Supports relative coordinates. |
| `/plot lock` | None | Locks the player into the plot so moving outside of the plot bounds does not transfer you to other plots. |
| `/plot unlock` | None | Reverses the locking done by `/plot lock`. |
| `/plot select` | `/p sel` | Uses WorldEdit to select the entire plot. |

### Worldedit
MCHPRS provides its own implementation of [WorldEdit](https://github.com/EngineHub/WorldEdit). Visit their [documentation](https://worldedit.enginehub.org/en/latest/commands/) for more information.
These are the commands that are currently implemented:
| Command | Alias | Description |
| --- | --- | --- |
| `/up` | `/u` | Go upwards some distance |
| `/ascend` | `/asc` | Go up a floor |
| `/descend` | `/desc` | Go down a floor |
| `//pos1` | `//1` | Set position 1 |
| `//pos2` | `//2` | Set position 2 |
| `//hpos1` | `//h1` | Set position 1 to targeted block |
| `//hpos2` | `//h2` | Set position 2 to targeted block |
| `//sel` | None | Clears your worldedit first and second positions. |
| `//set` | None | Sets all the blocks in the region |
| `//replace` | None | Replace all blocks in a selection with another |
| `//copy` | `//c` | Copy the selection to the clipboard |
| `//cut` | `//x` | Cut the selection to the clipboard |
| `//paste` | `//v` | Paste the clipboard's contents (`-a` to ignore air, `-u` to also update) |
| `//undo` | None | Undoes the last action (from history) |
| `//redo` | None | Redoes the last action (from history) |
| `//rstack` | `//rs` | Stack with more options, Refer to [RedstoneTools](https://github.com/paulikauro/RedstoneTools) |
| `//stack` | `//s` | Repeat the contents of the selection |
| `//move` | None | Move the contents of the selection |
| `//count` | None | Counts the number of blocks matching a mask |
| `//load` | None | Loads a schematic from the `./schems/` folder. Make sure the schematic in the Sponge format if there are any issues. |
| `//save` | None | Save a schematic to the `./schems/` folder. |
| `//expand` | `//e` | Expand the selection area |
| `//contract` | None | Contract the selection area |
| `//shift` | None | Shift the selection area |
| `//flip` | `//f` | Flip the contents of the clipboard across the origin |
| `//rotate` | `//r` | Rotate the contents of the clipboard |
| `//update` | None | Updates all blocks in the selection (`-p` to update the entire plot) |
| `//help` | None | Displays help for WorldEdit commands |

### Redpiler

MCHPRS provides Redpiler, the redstone compiler. This allows redstone simulation much faster than otherwise possible.
While redpiler is running, all redstone connections are pre-computed, thus interaction with the world is limited in this state.
Placing or breaking blocks while redpiler is running will cause a reset and disable redpiler.

| Command | Alias | Description |
| --- | --- | --- |
| `/redpiler compile` | `/rp c` | Manually starts redpiler compilation. There are several flags available, described below. |
| `/redpiler reset` | `/rp r` | Stops redpiler. |

| Flag | Short | Description |
| --- | --- | --- |
| `--optimize` | `-o` | Enable redpiler optimizations. WARNING: This can, and will, break the state of your build. Use backups when using this flag. |
| `--io-only` | `-i` | Only send blocks updates of relavent input/output blocks. This includes trapdoors, lamps, note blocks, buttons, levers, and pressure plates. Using this flag can significantly reduce lag and improve simulation speed. |
| `--wire-dot-out` | `-d` | Consider wires in the dot shape as an output block for `-i`. Useful for e.g. color displays. |
| `--update` | `-u` | Update all blocks after redpiler resets. |
| `--export` | `-e` | Export the compile graph using a binary format. This can be useful for developing out-of-tree uses of redpiler graphs. |
| `--export-dot` | None | Create a graphvis dot file of backend graph. Used for debugging/development. |

## Acknowledgments
- [@AL1L](https://github.com/AL1L) for his contributions to worldedit and other various features.
- [@DavidGarland](https://github.com/DavidGarland) for a faster and overall better implementation of `get_entry` in the in-memory storage. This simple function runs 30% of the runtime for redstone.

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## License
[MIT](https://choosealicense.com/licenses/mit/)
