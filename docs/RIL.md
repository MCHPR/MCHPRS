# Redstone Intermediate Language

## Syntax

### Circuits

```
circuit @<name> {}
backend_circuit(<backend_name>) @<name> {}
```

Backends each have their own printer and parser to suit their own needs

### Schematics

```
schematic @<name> <path>
```

`path` is a string path relative to the RIL file. If the file is stdin, the path is relative to the working directory.

### Tests

`test_args <args>` specifies the default `redpiler_args` used in test blocks. `args` is a string.

`test(@<src>) <test_result>` or `test(@<src>, <redpiler_args>) <test_result>`

`redpiler_args` is a string. If `redpiler_args` is not specified, it must be specified through `test_args`.

`test_result` can either be a `circuit` or `backend_circuit`. `test_result` can be automatically updated with the `--update-tests` flag.

```
test_args <args>
test(@<src>) circuit @<test_name> {}
test(@<src>) backend_circuit(<backend_name>) @<test_name> {}
```

### Values

Values are a name given to a component prefixed with `%`. They can include any alphanumeric characters.

Examples: `%a`, `%repeater`, `%123`

### Input lists

Input lists are defined as a list of value and distance pairs. The distance must be between 0 and 15 inclusive.

`[%<name>:<distance>, ...]`

For example: `[%a:12, %12:2]`

## Components

```
%x = repeater <delay>, <facing_diode>, <locked>, <powered>, <base_inputs>, <side_inputs>
%x = torch <powered>, <inputs>
%x = comparator <mode>, <far_input>, <facing_diode>, <output_strength>, <base_inputs>, <side_inputs>
%x = lamp <powered>, <inputs>
%x = button <powered>
%x = lever <powered>
%x = pressure_plate <powered>
%x = trapdoor <powered>, <inputs>
%x = wire <output_strength>, <inputs>
%x = constant <output_strength>
%x = note_block <instrument>, <note>, <inputs>
```
