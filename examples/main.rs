use reinda::Assets;


// This embeds files into the executable, ... at least in prod mode. In dev
// mode, all files are loaded dynamically at runtime. You can also have assets
// that are loaded always at runtime: these are not mentioned in this macro,
// but added below to the builder.
const EMBEDS: reinda::Embeds = reinda::embed! {
    print_stats: true,
    // compression_quality: 8,
    // compression_threshold: 0.7,
    base_path: "examples/assets",
    files: [
        "index.html",
        "robots.txt",
        "logo.svg",
        "style.css",

        // Use wildcard to include multiple files
        "fonts/*.woff2",

        // Or in this case, we use the wildcard since we don't know the exact
        // filename. For example, it might contain a hash calculated by webpack
        // or some other bundler.
        "bundle.*.js",
        "bundle.*.js.map",
    ],
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Here we just print information from `EMBEDS`
    println!("--- Entries in embeds:");
    for entry in EMBEDS.entries() {
        println!(
            "'{}' -> {:?}",
            entry.path_pattern(),
            entry.files().map(|f| f.path()).collect::<Vec<_>>(),
        );
    }
    println!();


    // ------------------------------------------------------
    // Time to collect and prepare all assets. Use a builder to configure everything.
    let mut builder = Assets::builder();

    // The most simple case: the asset is mounted with the same HTTP path as the
    // path with which it was included.
    builder.add_embedded("robots.txt", &EMBEDS["robots.txt"]);

    // But the HTTP path can also differ from the path it was included by.
    builder.add_embedded("img/logo-foo.svg", &EMBEDS["logo.svg"]);

    // You can also add assets that have not been embedded, but are loaded at
    // runtime.
    builder.add_file("dummy/exe", std::env::current_exe().unwrap());

    // For glob entries, the first argument is the HTTP prefix/base. The
    // resulting path is that plus all the segments from the pattern that
    // contain glob metacharacters. In this case, it will be
    // `static/font/open-sans/latin-400.woff2` for example.
    //
    // We also use a hashed filename here for caching. Finally we call
    // `http_paths` on the entry builder to get all the mounted paths for
    // later.
    let font_paths = builder.add_embedded("static/font/open-sans/", &EMBEDS["fonts/*.woff2"])
        .with_hash()
        .http_paths();

    // Here we include the CSS file which refers to the fonts. The paths inside
    // the CSS file need to be fixed (the hash needs to be inserted). Since we
    // don't need to change anything else, `with_path_fixup` works well here.
    //
    // Note that this CSS file is also configured `with_hash`.
    let css_path = builder.add_embedded("static/style.css", &EMBEDS["style.css"])
        .with_path_fixup(font_paths)
        .with_hash()
        .single_http_path()
        .unwrap();

    // Mounting our JS bundles. They don't need `with_hash()` since the hash was
    // already calculated by webpack (well, lets pretend it was).
    let bundle_path = builder.add_embedded("static/", &EMBEDS["bundle.*.js"])
        .single_http_path()
        .unwrap();
    builder.add_embedded("static/", &EMBEDS["bundle.*.js.map"]);

    // Now the most complex thing: index.html which refers to different files
    // and needs other adjustments. For that we use a custom "modifier".
    let dependencies = [css_path.clone(), bundle_path.clone()];
    builder.add_embedded("index.html", &EMBEDS["index.html"])
        .with_modifier(dependencies, move |original, ctx| {
            // We want to fixup two paths, but also replace a variable
            // `{{ foo }}` with a value. We can use `util::replace_many` to
            // perform multiple replacements.
            reinda::util::replace_many(&original, &[
                (css_path.as_ref(), ctx.resolve_path(&css_path)),
                ("{{ bundle_path }}", ctx.resolve_path(&bundle_path)),
                ("{{ foo }}", "foxes are best"),
            ]).into()
        });


    // ------------------------------------------------------
    // Everything is configured, so now lets prepare the assets. In prod mode,
    // this entails loading all files that weren't embedded, and applying all
    // modifiers and calculating all hashes, to end up with basically
    // `HashMap<String, Bytes>`. In dev mode, not much is done, but the loading
    // and applying the modifier are done on the fly when an asset is
    // requested.
    let assets = builder.build().await?;


    // ------------------------------------------------------
    // Now lets use the prepared assets. Note: try running this example in prod
    // and dev mode to see the difference!

    let index_html = assets.get("index.html").unwrap();
    assert!(!index_html.is_filename_hashed());
    let index_content = index_html.content().await?;
    let index_content = std::str::from_utf8(&index_content).unwrap();
    assert!(index_content.contains(r#"<script src="/static/bundle.8f29ad31.js" />"#));
    assert!(index_content.contains(
        r#"<script>console.log("Secret variable: foxes are best");</script>"#));
    println!("--- index.html:\n{index_content}---\n");

    let (css_path, style_css) = assets.iter().find(|(path, _)| path.ends_with(".css")).unwrap();
    assert_eq!(
        style_css.is_filename_hashed(),
        cfg!(any(feature = "always-embed", not(debug_assertions))),
    );
    let style_content = style_css.content().await?;
    let style_content = std::str::from_utf8(&style_content).unwrap();
    println!("--- {css_path}:\n{style_content}---\n");

    println!("--- All asset paths:");
    for (path, _) in assets.iter() {
        println!("{path}");
    }


    Ok(())
}
