# `reinda`: easily embed and manage assets

[<img alt="CI status of master" src="https://img.shields.io/github/actions/workflow/status/LukasKalbertodt/reinda/ci.yml?branch=master&label=CI&logo=github&logoColor=white&style=for-the-badge" height="23">](https://github.com/LukasKalbertodt/reinda/actions/workflows/ci.yml)
[<img alt="Crates.io Version" src="https://img.shields.io/crates/v/reinda?logo=rust&style=for-the-badge" height="23">](https://crates.io/crates/reinda)
[<img alt="docs.rs" src="https://img.shields.io/crates/v/reinda?color=blue&label=docs&style=for-the-badge" height="23">](https://docs.rs/reinda)

This library helps you manage your assets (external files) and is mostly
intended to be used in web applications. Assets can be embedded into the binary
file to obtain an easy to deploy standalone executable. This crate also offers a
template engine and the ability to automatically include a hash of an asset's
content into its filename (useful for caching on the web). In release mode, this
crate prepares everything up-front such that the actually serving the file via
HTTP can be as fast as possible.

You might know the crate `rust-embed`: `reinda` does basically the same, but for
the most part has more features and is more flexible (in my opinion).

**Tiny example**:

```rust
use reinda::{assets, Assets, Config, Setup};

const ASSETS: Setup = assets! {
    #![base_path = "assets"]

    "index.html": { template },
    "bundle.js": { hash },
};


let assets = Assets::new(ASSETS, Config::default()).await?;

// Retrieve specific asset. You can now send this data via HTTP or use it however you like.
let bytes /* : Option<bytes::Bytes> */ = assets.get("index.html")?;
```

See [**the documentation**](https://docs.rs/reinda) for more information.

## Features

- [x] Embed files at compile time (in prod mode) or load them at runtime (in dev mode)
- [x] Allow for `dynamic` files which will always be loaded at runtime
- [x] Include content hash in filename (has to be enabled per asset)
- [x] Template system
  - [x] Include other assets
  - [x] Refer to other assets by path
  - [x] Use runtime variables
- [x] Compress embedded files to shrink the resulting binary
- [x] Cargo feature to embed everything in debug mode
- [x] Template syntax checked at compile time (in prod mode)
- [ ] Well tested


## Status of this project

This project is very young. I developed it specifically for a web application I
work on where `rust-embed` did not offer enough features. You should absolutely
not use this in production yet, but you sure can try it out.

If you have any thoughts about this project, please let me know in [this
community feedback issue](https://github.com/LukasKalbertodt/reinda/issues/10)!


<br />

---

## License

Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
