# kompo-vfs

A virtual filesystem library written in Rust for the [kompo](https://github.com/ahogappa/kompo) gem. This library enables Ruby scripts and their dependencies to be packed into a single binary by providing a virtual filesystem layer that intercepts system calls.

## Overview

kompo-vfs consists of three main components:

| Crate | Description |
|-------|-------------|
| `kompo_fs` | Core virtual filesystem: the interposed libc entry points and the Ruby bindings |
| `kompo_tree` | Path index and file table over the embedded data |
| `kompo_wrap` | System call wrapper that intercepts and redirects filesystem operations |

`kompo_storage` is the previous trie-based index. Nothing links it any more; it is kept so `cargo bench -p kompo_tree` can measure the two side by side on the same image.

`kompo_tree` replaced it because path lookup through the trie cost more than the real filesystem it stands in for. It indexes whole paths in one hash and gives each directory a contiguous range of entries, so `readdir` neither searches nor allocates. Paths are handled as bytes end to end, which also means a filename that is not valid UTF-8 is looked up rather than panicked on.

### How It Works

kompo-vfs hooks into system calls (`open`, `read`, `stat`, `opendir`, etc.) to transparently redirect file operations. When the packed binary runs:

1. The virtual filesystem is initialized with embedded file data
2. System calls are intercepted and checked against the virtual filesystem
3. If the path exists in the VFS, the operation is handled internally
4. Otherwise, the call falls through to the real filesystem

## Installation

### Homebrew (Recommended)

```sh
$ brew tap ahogappa/kompo-vfs https://github.com/ahogappa/kompo-vfs.git
$ brew install ahogappa/kompo-vfs/kompo-vfs
```

### Building from Source

Prerequisites:
- [Rust](https://rustup.rs/) (stable)

```sh
$ git clone https://github.com/ahogappa/kompo-vfs.git
$ cd kompo-vfs
$ cargo build --release
```

The built static libraries will be at:
- `target/release/libkompo_fs.a`
- `target/release/libkompo_wrap.a`

## Usage

This library is designed to be used with the [kompo](https://github.com/ahogappa/kompo) gem. See the kompo documentation for details on packing Ruby applications into single binaries.

## Supported Platforms

| Platform | Status |
|----------|--------|
| macOS (ARM) | ✅ Supported |
| macOS (x64) | ❓ Untested |
| Linux (x64) | ✅ Supported |
| Linux (ARM) | ❓ Untested |
| Windows | 🚧 Not yet supported |

## Development

### Running Tests

```sh
$ cargo test -p kompo_storage -p kompo_fs -p kompo_tree
```

### Project Structure

```
kompo-vfs/
├── kompo_fs/           # Core VFS implementation
│   └── src/
│       ├── lib.rs      # Startup, Ruby bindings
│       ├── glue.rs     # Interposed libc entry points
│       └── util.rs     # Path resolution
├── kompo_tree/         # Path index and file table
│   ├── src/
│   │   ├── lib.rs      # Fs API: open, read, stat, readdir
│   │   └── tree.rs     # Node arena and path resolution
│   └── benches/
│       └── tree_bench.rs # A/B against kompo_storage
├── kompo_storage/      # Previous trie index, kept for the benchmark
│   └── src/
│       └── lib.rs      # File/directory data management
├── kompo_wrap/         # System call wrappers
│   └── src/
│       └── lib.rs      # Intercepts open, read, stat, etc.
└── Formula/            # Homebrew formula
```

## Contributing

Bug reports and pull requests are welcome on GitHub at https://github.com/ahogappa/kompo-vfs.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Related Projects

- [kompo](https://github.com/ahogappa/kompo) - Ruby gem that uses kompo-vfs to pack Ruby applications into single binaries
