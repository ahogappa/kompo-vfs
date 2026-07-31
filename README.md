# kompo-vfs

A virtual filesystem library written in Rust for the [kompo](https://github.com/ahogappa/kompo) gem. This library enables Ruby scripts and their dependencies to be packed into a single binary by providing a virtual filesystem layer that intercepts system calls.

## Overview

kompo-vfs consists of three main components:

| Crate | Description |
|-------|-------------|
| `kompo_fs` | Core virtual filesystem implementation using a trie data structure for efficient path lookup |
| `kompo_storage` | Storage layer that manages file data and directory entries |
| `kompo_wrap` | System call wrapper that intercepts and redirects filesystem operations |

Plus one crate that is not wired into the binary yet:

| Crate | Description |
|-------|-------------|
| `kompo_tree` | Flat-arena replacement for `kompo_storage`'s trie. Same operations, not yet used by `kompo_fs` |

`kompo_tree` exists because path lookup through the trie costs more than the real filesystem it replaces. It indexes whole paths in one hash and gives each directory a contiguous range of entries, so `readdir` neither searches nor allocates. `cargo bench -p kompo_tree` runs both side by side on the same image.

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
$ cargo test -p kompo_storage -p kompo_fs
```

### Project Structure

```
kompo-vfs/
├── kompo_fs/           # Core VFS implementation
│   └── src/
│       └── lib.rs      # Trie-based filesystem, Ruby bindings
├── kompo_storage/      # Storage layer
│   └── src/
│       └── lib.rs      # File/directory data management
├── kompo_tree/         # Flat-arena path index (not yet wired in)
│   ├── src/
│   │   ├── lib.rs      # Fs API: open, read, stat, readdir
│   │   └── tree.rs     # Node arena and path resolution
│   └── benches/
│       └── tree_bench.rs # A/B against kompo_storage
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
