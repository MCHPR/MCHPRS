# Adding a block

This document explains the process of adding a new block to MCHPRS.

### Editing the [gen_info.yaml](../mc_data/gen_info.yaml) file

Blocks and items in MCHPRS are defined in the [gen_info.yaml](../mc_data/gen_info.yaml). See the comments in this file for information on how to define a block and its corresponding item.

### Implementing placement logic for complex blocks

If a block has properties or otherwise has complex placement logic, you cannot use the `simple_item` attribute in `gen_info.yaml`. Therefore, the placement logic has to be implemented by hand in the [`core::interaction`](../crates/core/src/interaction.rs) module. Add a match arm for your `Item` variant in the `get_state_for_placement` function. See the other item to block mappings for an example.

### Running the block data generator

Running the block data generator will produce the [generated.rs](../crates/blocks/src/generated.rs) file containing the `Block` and `Item` enums.

```sh
cd crates/block_data_gen
cargo run
```
