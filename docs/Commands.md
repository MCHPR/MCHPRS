# MCHPRS Commands

## Table of Contents

- [General Commands](#general-commands)
- [Plot Commands](#plot-commands)
- [WorldEdit Commands](#worldedit-commands)
  - [Navigation](#navigation)
  - [Selection](#selection)
  - [Region Operations](#region-operations)
  - [Clipboard](#clipboard)
  - [History](#history)
  - [Schematics](#schematics)
  - [Other](#other)
- [Redpiler Commands](#redpiler-commands)
- [Command Aliases](#command-aliases)
- [Argument Types Reference](#argument-types-reference)
  - [Specialized Types](#specialized-types)

## General Commands

| Command | Alias | Description |
| --- | --- | --- |
| `/help [<command>]` | `//help` | List available commands, or show syntax and flags for a command. |
| `/speed <speed>` | None | Set your flying speed. |
| `/teleport (<position> \| <player>)` | `/tp` | Teleport to coordinates or another player. |
| `/rtps [unlimited \| <tps>]` | `/rtps u` (for unlimited) | Show plot timings or set the redstone tick rate (default 10 ticks per second). `0` pauses simulation; `unlimited` runs as fast as possible. One redstone tick equals two game ticks. |
| `/radvance <ticks>` | `/radv` | Immediately advance the plot by the given number of redstone ticks. |
| `/stop` | None | Stop the server. |
| `/gamemode (creative \| spectator)` | `1` (creative), `3` (spectator), `/gmc`, `/gmsp` | Change your game mode. |
| `/toggleautorp` | None | Toggle automatic Redpiler compilation for the current plot. |
| `/worldsendrate [<hertz>]` | `/wsr` | Show or set how often the plot sends block updates to players (default 60 Hz). |
| `/container <type> <power>` | None | Give yourself a barrel, furnace, or hopper filled to produce the specified comparator signal strength (0–15). |
| `/whitelist add <username>` | None | Add a player to the whitelist. |
| `/whitelist remove <username>` | None | Remove a player from the whitelist. |
| `/version` | None | Show the server version. |

## Plot Commands

Note: The plot ownership system in MCHPRS is very incomplete.

| Command | Alias | Description |
| --- | --- | --- |
| `/plot info` | `/p i` | Show the owner of your current plot. |
| `/plot claim` | `/p c` | Claim your current plot if it is unclaimed. |
| `/plot auto` | `/p a` | Find and claim an unclaimed plot. |
| `/plot middle` | None | Teleport to the center of your current plot. |
| `/plot visit <username> [<index>]` | `/p v` | Visit a player's plot. Use `index` to choose among their plots; defaults to the first. |
| `/plot teleport <location>` | `/p tp` | Teleport to a plot by its X Z coordinates. Relative `~` offsets are measured in plots. |
| `/plot lock` | None | Prevent crossing a plot boundary from transferring you to another plot. |
| `/plot unlock` | None | Re-enable transfers to neighboring plots when you cross their boundaries. |
| `/plot select` | `/p sel` | Select the entire plot with WorldEdit. |

## WorldEdit Commands

MCHPRS provides its own implementation of [WorldEdit](https://github.com/EngineHub/WorldEdit).
Visit their [documentation](https://worldedit.enginehub.org/en/latest/commands/) for more information.

### Navigation

| Command | Alias | Description |
| --- | --- | --- |
| `/jumpto` | `/j` | Teleport onto the block you are looking at. |
| `/unstuck` | `/!` | Escape from blocks by moving upward to a space with room to stand. |
| `/up [<distance>]` | `/u` | Move up by the given number of blocks (default 1), placing glass beneath you if needed. Flags: `-f`/`--force-flight` (enable flight instead), `-g`/`--force-glass` (also place glass when forcing flight). |
| `/ascend [<levels>]` | `/asc` | Teleport to the floor above, or move up several floors. |
| `/descend [<levels>]` | `/desc` | Teleport to the floor below, or move down several floors. |

### Selection

| Command | Alias | Description |
| --- | --- | --- |
| `//pos1 [<coordinates>]` | `//1` | Set the first selection corner to the given coordinates, or your current position. |
| `//pos2 [<coordinates>]` | `//2` | Set the second selection corner to the given coordinates, or your current position. |
| `//hpos1` | `//h1` | Set the first selection corner to the block you are looking at. |
| `//hpos2` | `//h2` | Set the second selection corner to the block you are looking at. |
| `//pos [<pos1> [<pos2>]]` | None | Set one or both selection corners. Without arguments, set the first corner to your current position. |
| `//sel` | `/;`, `//desel`, `//deselect` | Clear both selection corners. |
| `//expand (vert \| [<amount>] [<reverseAmount>] [<direction>])` | `//e` | Extend the selection in the given direction; `reverseAmount` also extends the opposite side. Defaults to 1 block in the direction you are facing. `vert` selects the full world height. |
| `//contract [<amount>] [<reverseAmount>] [<direction>]` | None | Shrink the selection toward the given direction; `reverseAmount` also shrinks it in the opposite direction. Defaults to 1 block in the direction you are facing. |
| `//shift [<amount>] [<direction>]` | None | Move the selection without moving its blocks. Defaults to 1 block in the direction you are facing. |

### Region Operations

| Command | Alias | Description |
| --- | --- | --- |
| `//set <pattern>` | None | Fill the selection with the specified block pattern. |
| `//replace [<mask>] <pattern>` | `//re`, `//rep` | Replace blocks matching the mask with the given pattern. Without a mask, replace non-air blocks in the selection. |
| `//count <mask>` | None | Count blocks in the selection that match the mask. |
| `//distr [-c]` | None | Show block counts and percentages in the selection. Flags: `-c`/`--clipboard` (use the clipboard instead). |
| `//stack [<count>] [<direction>]` | `//s` | Create adjacent copies of the selection. Defaults to 1 copy in the direction you are facing, including air. Flags: `-a`/`--ignore-air` (skip air); `-s`/`--shift-selection` (select the last copy). |
| `//move [<count>] [<direction>]` | None | Move the selected blocks, leaving air behind. Defaults to 1 block in the direction you are facing; parts moved outside the plot are clipped. Flags: `-a`/`--ignore-air` (skip air when placing blocks), `-s`/`--shift-selection` (move the selection with the blocks). |
| `//rstack [<count>] [<direction>] [<offset>]` | `//rs` | Repeat the selection at a fixed block offset, allowing copies to overlap. Supports diagonal directions. Defaults to 1 copy, 2 blocks in the direction you are looking, skipping air. Flags: `-w`/`--with-air` (include air); `-s`/`--shift-selection` (select the last copy) or `-e`/`--expand-selection` (include all copies in the selection). |

### Clipboard

| Command | Alias | Description |
| --- | --- | --- |
| `//copy` | `//c` | Copy selected blocks to your clipboard. |
| `//cut` | `//x` | Copy selected blocks to your clipboard, then replace them with air. |
| `//paste` | `//v` | Paste the clipboard relative to your current position. Flags: `-a`/`--ignore-air` (skip air), `-u`/`--update` (update pasted blocks), `-s`/`--select-region` (select the pasted region), `-n`/`--no-paste` (select the destination without pasting). |
| `//flip [<direction>]` | `//f` | Mirror the clipboard in the given direction, or the direction you are facing. |
| `//rotate <angle>` | `//r` | Rotate the clipboard around the vertical axis in 90° steps. |

### History

| Command | Alias | Description |
| --- | --- | --- |
| `//undo [<times>]` | `/undo` | Undo the last WorldEdit operation, or several operations. |
| `//redo [<times>]` | `/redo` | Redo the last undone WorldEdit operation, or several operations. |

### Schematics

The schematic command has multiple aliases: `//schematic`, `//schem`, `/schematic`, and `/schem`.

| Command | Alias | Description |
| --- | --- | --- |
| `//schematic list` | `//schem list`, `//schem all`, `//schem ls` | List available schematic files. |
| `//schematic load <filename>` | `//load` | Load a Sponge v2 or v3 schematic into your clipboard. |
| `//schematic save <filename>` | `//save` | Save your clipboard as a Sponge v2 schematic. Flags: `-f`/`--force-overwrite` (overwrite an existing file). |

Files are stored in `./schems/`, or `./schems/<uuid>/` when `schemati` is enabled.
Use filenames relative to that folder.

### Other

| Command | Alias | Description |
| --- | --- | --- |
| `//wand` | None | Give yourself a wooden axe for selecting corners with left and right clicks. |
| `//update` | None | Recalculate redstone in the selection. Flags: `-p`/`--plot` (update the entire plot). |
| `//replacecontainer [<from>] <to>` | `//rc` | Replace barrels, furnaces, or hoppers in the selection with another of these types, preserving comparator signal strength. Omit `from` to replace all three types. |

## Redpiler Commands

Redpiler compiles the plot's redstone circuits for faster simulation.
Placing or breaking blocks stops Redpiler and returns the plot to normal redstone ticking.

| Command | Alias | Description |
| --- | --- | --- |
| `/redpiler compile` | `/rp c` | Compile the current plot for accelerated redstone simulation. |
| `/redpiler inspect` | `/rp i` | Show the compiled node for the block you are looking at in the server's debug log. |
| `/redpiler reset` | `/rp r` | Stop compiled simulation and return to normal redstone ticking. |

### Compiler Flags

The `/redpiler compile` command supports these optional flags:

| Flag | Short | Description |
| --- | --- | --- |
| `--optimize` | `-o` | Enable Redpiler optimizations. This can and will break the state of your build; use backups. |
| `--io-only` | `-i` | Only send block updates for inputs and outputs: buttons, levers, pressure plates, lamps, trapdoors, and note blocks. This can reduce lag and improve simulation speed. |
| `--wire-dot-out` | `-d` | Treat redstone dots as outputs with `--io-only`, useful for color displays. |
| `--illegal-states-out` | `-l` | Treat redstone wires in illegal states (one connected side) as outputs with `--io-only`, useful for sprite displays. |
| `--wire-cross-out` | `-c` | Treat redstone crosses as outputs with `--io-only`, useful for sprite displays without illegal wire states. |
| `--update` | `-u` | Update all blocks after Redpiler resets. |
| `--export` | `-e` | Export the compiled graph in binary format for use by external tools. |
| `--export-dot` | None | Export the backend graph in Graphviz DOT format for debugging. |
| `--print-after-all` | None | Print the RIL circuit after each compiler pass. |
| `--print-before-backend` | None | Print the RIL circuit before backend compilation. |

## Command Aliases

MCHPRS command aliases can supply preset arguments and flags and control where your arguments are inserted.

Built-in aliases:

| Alias | Equivalent Command | Description |
| --- | --- | --- |
| `/gmc` | `/gamemode creative` | Change to creative mode. |
| `/gmsp` | `/gamemode spectator` | Change to spectator mode. |
| `//va` | `//paste -a` | Paste without air blocks. |
| `//sa` | `//stack {} -a` | Stack copies without air blocks. |
| `//load` | `//schematic load` | Load a schematic into your clipboard. |
| `//save` | `//schematic save` | Save your clipboard as a schematic. |

`{}` is replaced by everything you type after the alias.
For example, `//sa 5 north` runs `//stack 5 north -a`.

Server-specific aliases can be defined in [Config.toml](../README.md#custom-command-aliases).

## Argument Types Reference

### Specialized Types

| Type | Values | Description |
| --- | --- | --- |
| `<pattern>` | `stone`, `minecraft:oak_planks`, `repeater[delay=3]`, `stone,20%glass`, `^[powered=true]` | Blocks to place, optionally with states. Commas separate random choices; prefixes such as `20%` set relative weights (default `100%`). `^` preserves compatible existing states unless overridden. |
| `<mask>` | `stone,glass`, `repeater[delay=2,powered=true]`, `!stone`, `#existing`, `#solid`, `#surface`, `>stone`, `<stone`, `%50`, `^[powered=true]` | Select blocks by type, state, or condition. Block lists match any state unless states are given. `!` negates, `>` checks the block below, `<` checks the block above, `%` matches randomly, `^[...]` matches block states. Quote masks separated by spaces to require all of them to match, e.g. `"stone #solid"`. |
| `<type>` | `barrel`, `furnace`, `hopper` | Container type to use. |
| `<angle>` | `90`, `180`, `270`, `-90`, `-180` | Rotation in degrees, in 90° steps. |
| `<direction>` | `me`, `left`/`l`, `right`/`r`, `up`/`u`, `down`/`d`, `north`/`n`, `south`/`s`, `east`/`e`, `west`/`w` | A compass direction or a direction relative to where you are looking. Defaults to `me`, the direction you are facing. |
| `<direction>` of `//rstack` | All `<direction>` values plus diagonals: `lu`, `ld`, `ru`, `rd`, `nu`, `nd`, `su`, `sd`, `eu`, `ed`, `wu`, `wd` | Combine directions to stack diagonally, e.g. `leftup`/`lu` for left and up. |

State patterns such as `^[powered=true]` change only compatible blocks, leaving other blocks unchanged.
The mask `^[delay=2]` matches blocks with delay 2 and blocks without a delay property.
Use `^=[delay=2]` to require the property to exist and equal 2.
`#surface`/`#exposed` matches non-air blocks with an air neighbor.

Patterns and masks can use single or double quotes to include spaces.
Inside quotes, prefix the matching quote or a backslash with `\` to escape it.
Exact block state IDs, such as `=5922`, also work in block lists.
