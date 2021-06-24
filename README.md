# Minecraft High-Performance Redstone Server

[![Build Status](https://travis-ci.org/MCHPR/MCHPRS.svg?branch=master)](https://travis-ci.org/MCHPR/MCHPRS) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT) [![Discord Banner 2](https://discordapp.com/api/guilds/724072903083163679/widget.png)](https://discord.com/invite/svK9JU7)


A Minecraft creative server built for redstone. Each 256x256 plot runs on a separate thread, allowing for less lag, more concurrency, and many awesome extra features!

MCHPRS is very different from traditional servers. Because this server is tailored to the use of computation redstone, many things that are a part of Vanilla Minecraft servers don't exist here. That being said, MCHPRS comes with many of its own unique features.

## Goals


## Building

If the Rust compiler is not already installed, you can find out how [on their official website](https://www.rust-lang.org/tools/install).

```shell
git clone https://github.com/MCHPR/MCHPRS.git
cd MCHPRS
rustup override set nightly
cargo build --release
```

Once complete, the optimized executable will be located at `./target/release/mchprs` or `./target/release/mchprs.exe` depending on your operating system.

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

### Worldedit
MCHPRS provides its own implementation of [WorldEdit](https://github.com/EngineHub/WorldEdit). Visit their [documentation](https://worldedit.enginehub.org/en/latest/commands/) for more information.
These are the commands that are currently implemented:
| Command | Alias | Description |
| --- | --- |--- |
| `/up` | `/u` | Go upwards some distance |
| `//pos1` | `//1` | Set position 1 |
| `//pos2` | `//2` | Set position 2 |
| `//hpos1` | `//h1` | Set position 1 to targeted block |
| `//hpos2` | `//h2` | Set position 2 to targeted block |
| `//sel` | None | Clears your worldedit first and second positions. |
| `//set` | None | Sets all the blocks in the region |
| `//replace` | None | Replace all blocks in a selection with another |
| `//copy` | `//c` | Copy the selection to the clipboard |
| `//cut` | `//x` | Cut the selection to the clipboard |
| `//paste` | `//v` | Paste the clipboard's contents |
| `//undo` | None | Undo's the last action (from history) |
| `//stack` | `//s` | Repeat the contents of the selection |
| `//move` | None | Move the contents of the selection |
| `//count` | None | Counts the number of blocks matching a mask |
| `//load` | None | Loads a schematic from the `./schems/` folder. Make sure the schematic in the Sponge format if there are any issues. |
| `//save` | None | Save a schematic to the `./schems/` folder. |
| `//expand` | `//e` | Expand the selection area |
| `//contract` | None | Contract the selection area |
| `//shift` | None | Shift the selection area |
| `//help` | None | Displays help for WorldEdit commands |

## Acknowledgments
- [@AL1L](https://github.com/AL1L) for his contributions to worldedit and other various features.
- [@DavidGarland](https://github.com/DavidGarland) for a faster and overall better implementation of `get_entry` in the in-memory storage. This simple function runs 30% of the runtime for redstone.

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## License
[MIT](https://choosealicense.com/licenses/mit/)
