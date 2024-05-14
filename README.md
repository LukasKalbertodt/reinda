# `reinda`: easily embed and manage assets

[<img alt="CI status of main" src="https://img.shields.io/github/actions/workflow/status/LukasKalbertodt/reinda/ci.yml?branch=main&label=CI&logo=github&logoColor=white&style=for-the-badge" height="23">](https://github.com/LukasKalbertodt/reinda/actions/workflows/ci.yml)
[<img alt="Crates.io Version" src="https://img.shields.io/crates/v/reinda?logo=rust&style=for-the-badge" height="23">](https://crates.io/crates/reinda)
[<img alt="docs.rs" src="https://img.shields.io/crates/v/reinda?color=blue&label=docs&style=for-the-badge" height="23">](https://docs.rs/reinda)

This library helps your web applications manage your assets (external files).
Assets can be compressed and embedded into the binary file to obtain an easy to
deploy standalone executable. In debug mode, assets are loaded dynamically to
avoid having to recompile the backend. A hash can be automatically included in
an asset's filename to enable good caching on the web. In release mode, this
crate prepares everything up-front such that the actually serving the file via
HTTP can be as fast as possible.

You might know the crate `rust-embed`: `reinda` does basically the same, but for
the most part has more features and is more flexible (in my opinion).

**Tiny example**:

```rust
use reinda::{Assets, Embeds, embed};

// Embed some assets
const EMBEDS: Embeds = embed! {
    base_path: "../assets",
    files: ["index.html", "bundle.*.js"],
};

// Configure assets
let mut builder = Assets::build();
builder.add_embedded("index.html", &EMBEDS["index.html"]);
builder.add_embedded("static/", &EMBEDS["bundle.*.js"]);
let assets = builder.build().await?;

// Retrieve asset for serving. The `.await?` is only there for the "dev" mode
// when the file is dynamically loaded. In release mode, the final `Bytes`
// are already stored inside `assets`.
let bytes = assets.get("index.html").unwrap().content().await?;
```

See [**the documentation**](https://docs.rs/reinda) for more information.


## Status of this project

While this crate is not used by many projects yet, we use it in production for a
couple of years already. If you have any thoughts about this project, please
let me know in [this community feedback issue](https://github.com/LukasKalbertodt/reinda/issues/10)!


<br />

---

## License

Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
