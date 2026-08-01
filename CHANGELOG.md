# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0] - 2026-08-01

### Added
- Add `kompo_tree`, a flat-arena path index replacing the trie in `kompo_storage` [#24](https://github.com/ahogappa/kompo-vfs/pull/24)

### Changed
- Index the embedded image with `kompo_tree`: `stat` 4.0-15.7us -> ~11ns, `readdir` on a 40 entry directory 7.5ms -> 1.0us [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- Handle paths as bytes throughout, so a filename that is not valid UTF-8 is looked up rather than panicked on [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- Report inodes as `INO_BASE | node_id`, unique by construction and clear of the range a real filesystem uses [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- Reuse the `dirent` owned by the `DIR` in `readdir`, matching `readdir(3)` and no longer leaking 280 bytes per call [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- `kompo_storage` is no longer linked; it remains only as the benchmark baseline [#24](https://github.com/ahogappa/kompo-vfs/pull/24)

### Fixed
- Return a canonical path from `realpath` instead of the caller's spelling [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- Stop claiming sibling directories whose name extends the working directory's [#24](https://github.com/ahogappa/kompo-vfs/pull/24)
- Make `bump_version.sh` work with GNU sed as well as BSD sed [#25](https://github.com/ahogappa/kompo-vfs/pull/25)

## [0.6.0] - 2026-01-31

### Added
- Add zlib compression support for embedded files [#21](https://github.com/ahogappa/kompo-vfs/pull/21)

### Changed
- Implement internal RwLock for fd_map in Fs struct for improved thread safety [#20](https://github.com/ahogappa/kompo-vfs/pull/20)
- Add benchmarks for internal RwLock vs external Mutex [#20](https://github.com/ahogappa/kompo-vfs/pull/20)

## [0.5.1] - 2026-01-25

### Added
- Handle `O_DIRECTORY` flag in `open_from_fs` function [#18](https://github.com/ahogappa/kompo-vfs/pull/18)
- Add unit tests for `O_DIRECTORY` flag handling [#18](https://github.com/ahogappa/kompo-vfs/pull/18)

### Changed
- Add `VERSION` file as single source of truth for version management [#17](https://github.com/ahogappa/kompo-vfs/pull/17)
- Add `bump_version.sh` script to sync versions across files [#17](https://github.com/ahogappa/kompo-vfs/pull/17)

## [0.5.0] - 2026-01-24

### Added
- Enable PIC (Position Independent Code) for x86_64 Linux to support PIE binaries [#15](https://github.com/ahogappa/kompo-vfs/pull/15)
- Add panic=abort flag and version output for x86_64 Linux [#15](https://github.com/ahogappa/kompo-vfs/pull/15)

### Fixed
- Remove panic=abort from config.toml to fix test builds [#15](https://github.com/ahogappa/kompo-vfs/pull/15)

## [0.4.1] - 2026-01-23

### Changed
- Upgrade to Rust 2024 edition with thread-safe improvements [#13](https://github.com/ahogappa/kompo-vfs/pull/13)
- Refactor kompo_wrap to use thread-safe patterns
- Make convert_byte signature consistent across platforms

## [0.4.0] - 2026-01-21

### Added
- Add `kompo_fs_set_entrypoint_dir` function for setting entrypoint directory [#12](https://github.com/ahogappa/kompo-vfs/pull/12)
- Add unit tests for `kompo_fs_set_entrypoint_dir` function

## [0.3.0] - 2026-01-20

### Added
- Add `getattrlist` syscall support for macOS [#10](https://github.com/ahogappa/kompo-vfs/pull/10)
- Add comprehensive unit tests for kompo_storage and kompo_fs [#8](https://github.com/ahogappa/kompo-vfs/pull/8)
- Add kompo_fs tests to CI pipeline

### Fixed
- Fix macOS build issues [#9](https://github.com/ahogappa/kompo-vfs/pull/9)

## [0.2.0] - 2025-04-25

### Changed
- Refactor codebase [#6](https://github.com/ahogappa/kompo-vfs/pull/6)

### Fixed
- Fix formula file [#5](https://github.com/ahogappa/kompo-vfs/pull/5)

### Removed
- Delete START_FILE_PATH static variable [#4](https://github.com/ahogappa/kompo-vfs/pull/4)

## [0.1.0] - 2025-04-08

### Added
- Initial release with virtual filesystem implementation
- Use dlsym with RTLD_NEXT for syscall interception [#2](https://github.com/ahogappa/kompo-vfs/pull/2)
- Support for file operations: open, close, read, pread, realpath
- Support for directory operations: opendir, readdir, closedir, fdopendir
- Support for stat operations: stat, lstat, fstat, fstatat
- Ruby FFI bindings for kompo gem integration
