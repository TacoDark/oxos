# OxOS

OxOS is a hobby operating system written in Rust, designed for x86_64 PCs.  
It features a simple VGA text-based command line interface with basic commands.

## Features

- VGA text mode output
- Keyboard input with Shift and symbol support
- Simple command line with `echo` and `clear` commands
- Written in `no_std` Rust

## Building

You need [Rust nightly](https://rustup.rs/) and [bootimage](https://github.com/rust-osdev/bootimage):

```sh
rustup override set nightly
cargo install bootimage
```

Build the kernel image:

```sh
cargo bootimage -Z build-std=core,alloc --target x86_64-oxos.json
```

## Running

Run in QEMU:

```sh
qemu-system-x86_64 -drive format=raw,file=target/x86_64-oxos/debug/bootimage-oxos.bin
```

Actual Hardware:
🤷

## Usage

- Type `echo hello` to print `hello`
- Type `clear` to clear the screen
- Use Shift for uppercase and symbols

## License

MIT

---
OxOS is a learning project. Contributions and suggestions are reccomended!

## Known Issues

- **Typing Bug:** There is a known issue with the command line input where typing two of the same characters in a row, as well as using space and backspace, does not work correctly. This affects normal command entry and editing.
- Other features may be incomplete or unstable.