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