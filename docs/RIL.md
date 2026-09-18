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

Input lists are defined as a list of value and weight pairs. Weight is the power lost along a link. Values from 0 through 255 are accepted; the `clamp-weights` pass removes links with weight of 15 or more.

`[%<name>:<weight>, ...]`

For example: `[%a:12, %12:2]`

## Components

All components have a type and a list of arguments.

These arguments have a standard type:
- `powered`: boolean
- `base_inputs`, `side_inputs`, `inputs`: input list
- `power`: integer between 0 and 15

### repeater

```
%x = repeater <delay>, <facing_diode>, <locked>, <powered>, <base_inputs>, <side_inputs>
```

`delay` is an integer between 1 and 4.\
`facing_diode` is a boolean.\
`locked` is a boolean.

### torch

```
%x = torch <powered>, <inputs>
```

### comparator

```
%x = comparator <mode>, <far_input>, <facing_diode>, <power>, <base_inputs>, <side_inputs>
```

`mode` can either be `compare` or `subtract`.\
`far_input` can either be `none` or an integer between 0 and 15.\
`facing_diode` is a boolean.

### lamp

```
%x = lamp <powered>, <inputs>
```

### button

```
%x = button <powered>
```

### lever

```
%x = lever <powered>
```

### pressure_plate

```
%x = pressure_plate <powered>
```

### trapdoor

```
%x = trapdoor <powered>, <inputs>
```

### wire

```
%x = wire <power>, <inputs>
```

### constant

```
%x = constant <power>
```

### note_block

```
%x = note_block <instrument>, <note>, <inputs>
```

`instrument` is one of `harp`, `basedrum`, `snare`, `hat`, `bass`, `flute`, `bell`, `guitar`, `chime`, `xylophone`, `iron_xylophone`, `cow_bell`, `didgeridoo`, `bit`, `banjo`, or `pling`.\
`note` is an integer between 0 and 24.
