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
- [Custom Command Aliases](#custom-command-aliases)
  - [Built-in Custom Aliases](#built-in-custom-aliases)
  - [Adding Your Own Aliases](#adding-your-own-aliases)
- [Argument Types Reference](#argument-types-reference)
  - [Basic Types](#basic-types)
  - [Position Types](#position-types)
  - [Direction Types](#direction-types)
  - [WorldEdit Types](#worldedit-types)
  - [Special Types](#special-types)
  - [Command Flags](#command-flags)

## General Commands

| Command | Alias | Description |
| --- | --- | --- |
| `/help [<command>]` | None | Displays help for a specific command or lists all available commands. |
| `/speed <speed>` | None | Sets your fly speed. Range: `0.0 - 10.0`. |
| `/teleport (<position> \| <player>)` | `/tp` | Teleports you to the location or target. |
| `/rtps [unlimited \| <tps>]` | `/rtps u` (for unlimited) | Without arguments, displays current RTPS timings. With arguments, sets the redstone ticks per second to `unlimited` or a number. Default: 10. (There are two game ticks in a redstone tick) |
| `/radvance <ticks>` | `/radv` | Advances the plot by the specified number of redstone ticks. |
| `/stop` | None | Stops the server. |
| `/gamemode (creative \| spectator)` | `1` (creative), `3` (spectator), `/gmc`, `/gmsp` | Sets your gamemode to creative or spectator. |
| `/toggleautorp` | None | Toggles automatic redpiler compilation. |
| `/worldsendrate <hertz>` | `/wsr` | Sets the world send rate in hertz (frequency of world updates sent to clients). Range: `1 - 1000`. Default: `60`. |
| `/container <type> <power>` | None | Gives you a container which outputs the specified comparator power level when used with a comparator. Type must be one of: `barrel`, `furnace`, `hopper`. |
| `/whitelist add <username>` | None | Adds a player to the whitelist. |
| `/whitelist remove <username>` | None | Removes a player from the whitelist. |

## Plot Commands

Note: The plot ownership system in MCHPRS is very incomplete.

| Command | Alias | Description |
| --- | --- | --- |
| `/plot info` | `/p i` | Gets the owner of the plot you are in. |
| `/plot claim` | `/p c` | Claims the plot you are in if it is not already claimed. |
| `/plot auto` | `/p a` | Automatically finds an unclaimed plot and claims it. |
| `/plot middle` | None | Teleports you to the center of the plot you are in. |
| `/plot visit <username> [<index>]` | `/p v` | Teleports you to a player's plot. Defaults to their first plot. |
| `/plot tp <location>` | None | Teleports you to the plot at the specified plot coordinates (X Z format). |
| `/plot lock` | None | Locks you to the current plot so moving outside of the plot bounds does not transfer you to other plots. |
| `/plot unlock` | None | Unlocks you from the current plot. |
| `/plot select` | `/p sel` | Uses WorldEdit to select the entire plot. |

## WorldEdit Commands

MCHPRS provides its own implementation of [WorldEdit](https://github.com/EngineHub/WorldEdit). Visit their [documentation](https://worldedit.enginehub.org/en/latest/commands/) for more information.

### Navigation

| Command | Alias | Description |
| --- | --- | --- |
| `/jumpto` | `/j` | Teleport to the block you are looking at. |
| `/unstuck` | `/!` | Escape from being stuck inside a block by finding a safe location nearby. |
| `/up <distance>` | `/u` | Go upwards a specified distance. Flags: `-f`/`--force-flight` (force flight), `-g`/`--force-glass` (place glass platform). |
| `/ascend [<levels>]` | `/asc` | Go up one or multiple floors. |
| `/descend [<levels>]` | `/desc` | Go down one or multiple floors. |

### Selection

| Command | Alias | Description |
| --- | --- | --- |
| `//pos1 [<coordinates>]` | `//1` | Set position 1 to your current location or specified coordinates (X Y Z format). |
| `//pos2 [<coordinates>]` | `//2` | Set position 2 to your current location or specified coordinates (X Y Z format). |
| `//hpos1` | `//h1` | Set position 1 to the block you are looking at. |
| `//hpos2` | `//h2` | Set position 2 to the block you are looking at. |
| `//pos [<pos1>] [<pos2>]` | None | Set selection positions. First argument sets pos1, second sets pos2. |
| `//sel` | `/;`, `//desel`, `//deselect` | Clears your WorldEdit first and second positions. |
| `//expand (vert \| <amount> [<reverseAmount> [<direction>] \| <direction>])` | `//e` | Expand the selection area. Use `vert` to expand to world limits. |
| `//contract <amount> [<reverseAmount> [<direction>] \| <direction>]` | None | Contract the selection area by the specified amount. |
| `//shift <amount> [<direction>]` | None | Shift the selection area without moving its contents. |

### Region Operations

| Command | Alias | Description |
| --- | --- | --- |
| `//set <pattern>` | None | Sets all blocks in the region to the specified pattern. |
| `//replace [<mask>] <pattern>` | `//re`, `//rep` | Replace blocks in the selection. Without a mask, replaces all blocks. |
| `//count <mask>` | None | Counts the number of blocks matching a mask in the selection. |
| `//distr [-c]` | None | Get the block distribution in the selection or clipboard. Flags: `-c` (use clipboard). |
| `//stack [<count> [<offset> [<direction>] \| <direction>]]` | `//s` | Repeat the contents of the selection. Defaults to 1 copy in the direction you're facing. Flags: `-a`/`--ignore-air` (skip air), `-s`/`--shift-selection` (shift selection to last copy). |
| `//move <count> [<direction>]` | None | Move the contents of the selection by the specified distance. Flags: `-a`/`--ignore-air` (skip air), `-s`/`--shift-selection` (shift selection). |
| `//rstack <count> [<spacing> [<direction-ext>] \| <direction-ext> [<spacing>]]` | `//rs` | Stack with more options. Refer to [RedstoneTools](https://github.com/paulikauro/RedstoneTools). Default spacing is 2. Flags: `-w`/`--with-air` (include air), `-e`/`--expand-selection` (expand to stacked region). |

### Clipboard

| Command | Alias | Description |
| --- | --- | --- |
| `//copy` | `//c` | Copy the selection to the clipboard. |
| `//cut` | `//x` | Cut the selection to the clipboard. |
| `//paste` | `//v` | Paste the clipboard's contents. Flags: `-a`/`--ignore-air` (skip air), `-u`/`--update` (update blocks), `-o`/`--original-position` (paste at original pos), `-s`/`--select-region` (select pasted region), `-n`/`--no-paste` (select only). |
| `//flip [<direction>]` | `//f` | Flip the contents of the clipboard across the specified axis. Defaults to the direction you are facing. |
| `//rotate <angle>` | `//r` | Rotate the contents of the clipboard by the specified angle. Angle must be a multiple of 90 (e.g., 90, 180, 270, -90). |

### History

| Command | Alias | Description |
| --- | --- | --- |
| `//undo [<times>]` | `/undo` | Undoes the last action or multiple actions from history. |
| `//redo [<times>]` | `/redo` | Redoes the last undone action or multiple actions. |

### Schematics

The schematic command has multiple aliases: `//schematic`, `//schem`, `/schematic`, and `/schem`.

| Command | Alias | Description |
| --- | --- | --- |
| `//schematic list` | `//schem list`, `//schem all`, `//schem ls` | Lists all available schematics in the `./schems/` folder. |
| `//schematic load <filename>` | `//load` | Loads a schematic from the `./schems/` folder to your clipboard. Make sure the schematic is in the Sponge format if there are any issues. |
| `//schematic save <filename>` | `//save` | Saves your clipboard as a schematic to the `./schems/` folder. Flags: `-f`/`--force-overwrite` (overwrite existing file). |

### Other

| Command | Alias | Description |
| --- | --- | --- |
| `//wand` | None | Gives you a WorldEdit wand item for selecting positions. |
| `//update` | None | Updates all blocks in the selection. Flags: `-p`/`--plot` (update entire plot). |
| `//replacecontainer [<from>] <to>` | `//rc` | Replace containers in the selection while preserving comparator signal strength. |

## Redpiler Commands

MCHPRS provides Redpiler, the redstone compiler.
This allows redstone simulation much faster than otherwise possible.
While redpiler is running, all redstone connections are pre-computed, thus interaction with the world is limited in this state.
Placing or breaking blocks while redpiler is running will cause a reset and disable redpiler.

| Command | Alias | Description |
| --- | --- | --- |
| `/redpiler compile` | `/rp c` | Manually starts redpiler compilation. Several flags are available, see below. |
| `/redpiler inspect` | `/rp i` | Inspects the redpiler compilation state for the block you are looking at. |
| `/redpiler reset` | `/rp r` | Stops redpiler and resets compilation. |

### Compiler Flags

The `/redpiler compile` command supports several optional flags:

| Flag | Short | Description |
| --- | --- | --- |
| `--optimize` | `-o` | Enable redpiler optimizations. WARNING: This can break the state of your build. Use backups when using this flag. |
| `--io-only` | `-i` | Only send block updates for relevant input/output blocks. This includes trapdoors, lamps, note blocks, buttons, levers, and pressure plates. Using this flag can significantly reduce lag and improve simulation speed. |
| `--wire-dot-out` | `-d` | Consider wires in the dot shape as an output block for `--io-only`. Useful for e.g. color displays. |
| `--update` | `-u` | Update all blocks after redpiler resets. |
| `--export` | `-e` | Export the compile graph using a binary format. This can be useful for developing out-of-tree uses of redpiler graphs. |
| `--export-dot` | None | Create a graphviz dot file of backend graph. Used for debugging/development. |
| `--print-after-all` | None | Print out the RIL circuit after every redpiler pass. Used for debugging/development. |
| `--print-before-backend` | None | Print out the RIL circuit before starting backend compilation. Used for debugging/development. |

## Custom Command Aliases
MCHPRS supports **custom command aliases** that allow you to create shortcuts for frequently used commands.
Unlike regular aliases (which are just alternative names for a command), custom aliases can:
- Pre-fill arguments
- Combine commands with specific flags
- Use placeholders (`{}`) to insert arguments

### Built-in Custom Aliases

| Alias | Expands To | Description |
| --- | --- | --- |
| `/gmc` | `gamemode creative` | Shortcut for creative mode |
| `/gmsp` | `gamemode spectator` | Shortcut for spectator mode |
| `//va` | `//paste -a` | Paste without air blocks |
| `//sa` | `//stack {} -a` | Stack without air blocks (preserves count argument) |
| `//load` | `//schematic load` | Shortcut for loading schematics |
| `//save` | `//schematic save` | Shortcut for saving schematics |

**Note**: `{}` preserves arguments. Example: `/sa 5` -> `/stack 5 -a`

### Adding Your Own Aliases

Add to your `Config.toml`:

```toml
[command_aliases]
# Simple example command shortcut
"rcio" = "redpiler compile -io"
```

The initial `/` is implied for both the alias as well as its replacement.

Requires server restart after changes.

## Argument Types Reference

Standard argument types like `<integer>`, `<number>`, `<string>`, `<text>`, `<boolean>`, `<player>`, `<filename>`, and position types (`<position>`, `<location>`, `<coordinates>`) follow typical conventions. Positions support relative coordinates using the `~` prefix.

### Specialized Types

| Type | Values | Description |
| --- | --- | --- |
| `<pattern>` | `stone`, `redstone_block`, `air`, `minecraft:oak_planks` | Block name or ID for placement. |
| `<mask>` | `stone`, `redstone_block`, `air`, `minecraft:oak_planks` | Block filter. Identical to `pattern` for now. |
| `<type>` | `barrel`, `furnace`, `hopper` | Container type to use. |
| `<angle>` | `90`, `180`, `270`, `-90`, `-180` | Rotation angle. Must be a multiple of 90. |
| `<direction>` | `me`, `left`/`l`, `right`/`r`, `up`/`u`, `down`/`d`, `north`/`n`, `south`/`s`, `east`/`e`, `west`/`w` | Cardinal or relative direction. `me` = facing direction (default). |
| `<direction-ext>` | All `<direction>` values plus diagonals: `lu`, `ld`, `ru`, `rd`, `nu`, `nd`, `su`, `sd`, `eu`, `ed`, `wu`, `wd` | Extended directions with diagonal support (e.g., `leftup`/`lu` = left + up). |

### Command Flags

WorldEdit commands support optional flags in short (`-a`, `-s`) or long (`--ignore-air`, `--shift-selection`) form.
Multiple short flags can be combined (e.g., `-as`).
Common flags include: `-u`/`--update` (update blocks), `-a`/`--ignore-air` (skip air), `-s`/`--shift-selection` (shift selection), `-e`/`--expand-selection` (expand selection). Flags must appear at the end of commands.
