use reinda::Assets;


#[cfg(feature = "hash")]
macro_rules! assert_get {
    ($assets:expr, $http_path:expr, $hashed:expr, $expected:expr) => {
        let asset = $assets.get($http_path)
            .expect(concat!("asset '", $http_path, "' not found"));
        assert_eq!(asset.is_filename_hashed(), $hashed);
        let content = asset.content().await?;
        let expected = AsRef::<[u8]>::as_ref($expected);
        if content != expected {
            core::panic!("Assertion failed: content is unexpected!\n\
                --- expected:\n{}\n\
                --- actual:\n{}\n",
                std::str::from_utf8(&expected).unwrap_or("binary file"),
                std::str::from_utf8(&content).unwrap_or("binary file"),
            )
        }
    };
}


#[tokio::test]
async fn minimal() -> Result<(), Box<dyn std::error::Error>> {
    const EMBEDS: reinda::Embeds  = reinda::embed! {
        base_path: "tests/files",
        files: ["peter.txt"],
    };

    let mut builder = Assets::builder();
    builder.add_embedded("märchen.md", &EMBEDS["peter.txt"]);
    let a = builder.build().await?;

    assert_eq!(a.len(), 1);
    assert_eq!(a.iter().count(), 1);

    let (path, asset) = a.iter().collect::<Vec<_>>().remove(0);
    assert_eq!(path, "märchen.md");
    let expected = b"Peter und der Wolf.\n".as_slice();
    assert_eq!(asset.content().await?, expected);
    assert_eq!(asset.is_filename_hashed(), false);

    let asset = a.get("märchen.md").unwrap();
    assert_eq!(asset.content().await?, expected);
    assert_eq!(asset.is_filename_hashed(), false);

    assert!(a.get("märchen.md2").is_none());
    assert!(a.get("märchen.m").is_none());
    assert!(a.get("xmärchen.md").is_none());
    assert!(a.get("peter.txt").is_none());

    Ok(())
}

/// This is almost the same setup as in `examples/main.rs`.
#[tokio::test]
#[cfg(feature = "hash")]
async fn use_case_web() -> Result<(), Box<dyn std::error::Error>> {
    const EMBEDS: reinda::Embeds = reinda::embed! {
        base_path: "examples/assets",
        files: [
            "index.html",
            "robots.txt",
            "logo.svg",
            "style.css",
            "fonts/*.woff2",
            "bundle.*.js",
            "bundle.*.js.map",
        ],
    };
    let mut builder = Assets::builder();
    builder.add_embedded("robots.txt", &EMBEDS["robots.txt"]);
    builder.add_embedded("img/logo-foo.svg", &EMBEDS["logo.svg"]);
    let font_paths = builder.add_embedded("static/font/open-sans/", &EMBEDS["fonts/*.woff2"])
        .with_hash()
        .http_paths();
    let css_path = builder.add_embedded("static/style.css", &EMBEDS["style.css"])
        .with_path_fixup(font_paths)
        .with_hash()
        .single_http_path()
        .unwrap();
    let bundle_path = builder.add_embedded("static/", &EMBEDS["bundle.*.js"])
        .single_http_path()
        .unwrap();
    builder.add_embedded("static/", &EMBEDS["bundle.*.js.map"]);

    let dependencies = [css_path.clone(), bundle_path.clone()];
    builder.add_embedded("index.html", &EMBEDS["index.html"])
        .with_modifier(dependencies, move |original, ctx| {
            reinda::util::replace_many(&original, &[
                (css_path.as_ref(), ctx.resolve_path(&css_path)),
                ("{{ bundle_path }}", ctx.resolve_path(&bundle_path)),
                ("{{ foo }}", "foxes are best"),
            ]).into()
        });

    let assets = builder.build().await?;

    assert_eq!(assets.len(), 10);
    assert_eq!(assets.iter().count(), 10);
    assert!(assets.iter().all(|(path, _)| assets.get(path).is_some()));

    assert_get!(assets, "robots.txt", false,
        include_str!("../examples/assets/robots.txt"));
    assert_get!(assets, "img/logo-foo.svg", false,
        include_str!("../examples/assets/logo.svg"));
    assert_get!(assets, "static/bundle.8f29ad31.js", false,
        include_str!("../examples/assets/bundle.8f29ad31.js"));
    assert_get!(assets, "static/bundle.8f29ad31.js.map", false,
        include_str!("../examples/assets/bundle.8f29ad31.js.map"));

    // Prod
    #[cfg(prod_mode)]
    {
        assert!(assets.get("static/font/open-sans/latin-700.woff2").is_none());
        assert!(assets.get("static/font/open-sans/latin-400.woff2").is_none());
        assert!(assets.get("static/font/open-sans/latin-i400.woff2").is_none());
        assert!(assets.get("static/font/open-sans/latin-i700.woff2").is_none());
        assert!(assets.get("fonts/latin-700.woff2").is_none());
        assert!(assets.get("fonts/latin-400.woff2").is_none());
        assert!(assets.get("fonts/latin-i400.woff2").is_none());
        assert!(assets.get("fonts/latin-i700.woff2").is_none());

        assert_get!(assets, "static/font/open-sans/latin-700.dg2a9XQMeJqm.woff2", true,
            include_bytes!("../examples/assets/fonts/latin-700.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-400.gfQvt7EsDOBh.woff2", true,
            include_bytes!("../examples/assets/fonts/latin-400.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-i400.fM8_OgHPOq9X.woff2", true,
            include_bytes!("../examples/assets/fonts/latin-i400.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-i700.sbfNUtVcqxUK.woff2", true,
            include_bytes!("../examples/assets/fonts/latin-i700.woff2"));

        assert_get!(assets, "static/style.G6XSfH9qR-JM.css", true, concat!(
            "html, body {\n",
            "    margin: 0;\n",
            "    padding: 0;\n",
            "}\n",
            "\n",
            "body {\n",
            "    background-color: lightblue;\n",
            "}\n",
            "\n",
            "@font-face {\n",
            "    font-family: 'Open Sans';\n",
            "    font-style: italic;\n",
            "    font-weight: 400;\n",
            "    src: url(static/font/open-sans/latin-i400.fM8_OgHPOq9X.woff2) format('woff2');\n",
            "}\n",
            "@font-face {\n",
            "    font-family: 'Open Sans';\n",
            "    font-style: italic;\n",
            "    font-weight: 700;\n",
            "    src: url(static/font/open-sans/latin-i700.sbfNUtVcqxUK.woff2) format('woff2');\n",
            "}\n",
            "@font-face {\n",
            "    font-family: 'Open Sans';\n",
            "    font-style: normal;\n",
            "    font-weight: 400;\n",
            "    src: url(static/font/open-sans/latin-400.gfQvt7EsDOBh.woff2) format('woff2');\n",
            "}\n",
            "@font-face {\n",
            "    font-family: 'Open Sans';\n",
            "    font-style: normal;\n",
            "    font-weight: 700;\n",
            "    src: url(static/font/open-sans/latin-700.dg2a9XQMeJqm.woff2) format('woff2');\n",
            "}\n",
        ));

        assert_get!(assets, "index.html", false, concat!(
            r#"<!DOCTYPE html>"#, "\n",
            r#"<html lang="en">"#, "\n",
            r#"  <head>"#, "\n",
            r#"    <title>Reinda</title>"#, "\n",
            r#"    <script src="/static/bundle.8f29ad31.js" />"#, "\n",
            r#"    <link rel="stylesheet" href="/static/style.G6XSfH9qR-JM.css">"#, "\n",
            r#"    <script>console.log("Secret variable: foxes are best");</script>"#, "\n",
            r#"  </head>"#, "\n",
            r#"  <body></body>"#, "\n",
            r#"</html>"#, "\n",
        ));
    }

    // Dev
    #[cfg(dev_mode)]
    {
        assert_get!(assets, "static/font/open-sans/latin-700.woff2", false,
            include_bytes!("../examples/assets/fonts/latin-700.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-400.woff2", false,
            include_bytes!("../examples/assets/fonts/latin-400.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-i400.woff2", false,
            include_bytes!("../examples/assets/fonts/latin-i400.woff2"));
        assert_get!(assets, "static/font/open-sans/latin-i700.woff2", false,
            include_bytes!("../examples/assets/fonts/latin-i700.woff2"));

        // Without hashes, its unchanged.
        assert_get!(assets, "static/style.css", false,
            include_str!("../examples/assets/style.css"));

        assert_get!(assets, "index.html", false, concat!(
            r#"<!DOCTYPE html>"#, "\n",
            r#"<html lang="en">"#, "\n",
            r#"  <head>"#, "\n",
            r#"    <title>Reinda</title>"#, "\n",
            r#"    <script src="/static/bundle.8f29ad31.js" />"#, "\n",
            r#"    <link rel="stylesheet" href="/static/style.css">"#, "\n",
            r#"    <script>console.log("Secret variable: foxes are best");</script>"#, "\n",
            r#"  </head>"#, "\n",
            r#"  <body></body>"#, "\n",
            r#"</html>"#, "\n",
        ));
    }

    Ok(())
}

// TODO:
// - cyclic dependencies
// - missing dependencies (modifier asks for other path)
