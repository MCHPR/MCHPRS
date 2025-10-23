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
    - [Velocity](#velocity)
    - [LuckPerms](#luckperms)
- [Usage](#usage)
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

The following options are available at the toplevel (under no header):
| Field | Description | Default |
| --- | --- | --- |
| `bind_address` | Bind address and port | `0.0.0.0:25565` |
| `motd` | Message of the day | `"Minecraft High Performance Redstone Server"` |
| `chat_format` | How to format chat messages interpolating `username` and `message` with curly braces | `<{username}> {message}` |
| `max_players` | Maximum number of simultaneous players | `99999` |
| `view_distance` | Maximal distance (in chunks) between players and loaded chunks | `8` |
| `whitelist` | Whether or not the whitelist (in `whitelist.json`) should be enabled | `false` |
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

MCHPRS has basic support for LuckPerms with MySQL or MariaDB remote database storage. This implementation has no commands or interface and would have to be managed through LuckPerms running on a proxy (`/lpb`) or other server (`/lp`).

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
# The name of the server, used for server-specific permissions.
# See: https://luckperms.net/wiki/Context
server_context = "global"
```

## Usage

For a full list of commands, see [Commands](docs/Commands.md).

The following categories of commands are implemented:
- **General** - Server control, player teleportation, and gamemode
- **Plot** - Claiming and managing plots
- **WorldEdit** - Selection, region operations, clipboard, and schematics
- **Redpiler** - Redstone compiler operations

Use the `/help [<command>]` command in-game to get detailed information about any specific command.

## Acknowledgments
- [@AL1L](https://github.com/AL1L) for his contributions to worldedit and other various features.
- [@DavidGarland](https://github.com/DavidGarland) for a faster and overall better implementation of `get_entry` in the in-memory storage. This simple function accounts for 30% of the redstone runtime.

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## License
[MIT](https://choosealicense.com/licenses/mit/)
