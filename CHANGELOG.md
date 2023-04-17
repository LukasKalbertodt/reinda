# Changelog

All notable changes to this project will be documented in this file.


## [Unreleased]


## [0.2.1] - 2023-04-17
- Update dependencies (none of these are exposed in the API, so a minor version bump is sufficient).

## [0.2.0] - 2021-01-27
### Added
- Add Cargo-feature `debug-is-prod` which enables prod mode (embedding, hashes,
  ...) when compiling in debug mode, too.
- `Info::is_filename_hashed`
- `Assets::lookup`
- Add Cargo-feature `compress` (enabled by default) to compress embedded data to
  shrink binary size. Compression via `flate2`.

### Changed
- **Breaking**: Make field of `AssetId` private
- **Breaking**: Change `Assets::asset_info` to panic instead of returning `None`
- Potentially breaking: Make filename hashing optional with Cargo-feature `hash` which is enabled by default
- **Breaking**: `prepend` and `append` now take byte string literals instead of normal string literal
- Potentially breaking: Check template syntax and includes at compile time in prod mode.

### Fixed
- Fix bug in filename hashing (forgot to add `/` between parents and file)
- Fix bug in include resolution

## 0.1.0 - 2021-01-22
### Added
- Everything


[Unreleased]: https://github.com/LukasKalbertodt/reinda/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/LukasKalbertodt/reinda/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/LukasKalbertodt/reinda/compare/v0.1.0...v0.2.0
