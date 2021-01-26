use core::panic;

use reinda::{Assets, Config, Error};


macro_rules! assert_get {
    ($left:expr, $right:literal) => {
        assert_eq!($left.await?.as_deref(), Some($right as &[_]));
    };
}


#[tokio::test]
async fn minimal() -> Result<(), Box<dyn std::error::Error>> {
    const ASSETS: reinda::Setup  = reinda::assets! {
        #![base_path = "tests/files"]

        "peter.txt": {},
    };

    let a = Assets::new(ASSETS, Config::default()).await?;

    assert_eq!(a.asset_ids().count(), 1);
    assert_get!(a.get("peter.txt"), b"Peter und der Wolf.\n");

    let id = a.asset_ids().next().unwrap();
    let info = a.asset_info(id);
    assert_eq!(info.original_path(), "peter.txt");
    assert_eq!(info.public_path(), "peter.txt");
    assert_eq!(info.is_served(), true);
    assert_eq!(info.is_dynamic(), false);
    assert_eq!(info.is_filename_hashed(), false);

    Ok(())
}

#[tokio::test]
async fn subdir() -> Result<(), Box<dyn std::error::Error>> {
    const ASSETS: reinda::Setup  = reinda::assets! {
        #![base_path = "tests/files"]

        "include_sub.txt": { template },
        "sub/anna.txt": {},
    };

    let a = Assets::new(ASSETS, Config::default()).await?;

    assert_eq!(a.asset_ids().count(), 2);
    assert_get!(a.get("sub/anna.txt"), b"anna\n");
    assert_get!(a.get("include_sub.txt"), b"My favorite human is: anna\n\n");

    for id in a.asset_ids() {
        let info = a.asset_info(id);
        assert_eq!(info.public_path(), info.original_path());
        assert_eq!(info.is_served(), true);
        assert_eq!(info.is_dynamic(), false);
        assert_eq!(info.is_filename_hashed(), false);
    }

    Ok(())
}

#[tokio::test]
async fn complex_includes() -> Result<(), Box<dyn std::error::Error>> {
    const ASSETS: reinda::Setup  = reinda::assets! {
        #![base_path = "tests/files/complex_includes"]

        "root.txt": { template },
        "a.txt": { template },
        "b.txt": { template },
        "c.txt": { template },
        "foo.txt": {},
        "bar.txt": {},
    };

    let a = Assets::new(ASSETS, Config::default()).await?;

    assert_eq!(a.asset_ids().count(), 6);
    assert_get!(a.get("foo.txt"), b"foo\n");
    assert_get!(a.get("bar.txt"), b"bar\n");
    assert_get!(a.get("c.txt"), b"c(bar\n)\n");
    assert_get!(a.get("b.txt"), b"b(c(bar\n)\n)\n");
    assert_get!(a.get("a.txt"), b"a(foo\n)\na(bar\n)\n");
    assert_get!(a.get("root.txt"), b"x a(foo\n)\na(bar\n)\n y b(c(bar\n)\n)\n z\n");

    for id in a.asset_ids() {
        let info = a.asset_info(id);
        assert_eq!(info.public_path(), info.original_path());
        assert_eq!(info.is_served(), true);
        assert_eq!(info.is_dynamic(), false);
        assert_eq!(info.is_filename_hashed(), false);
    }

    Ok(())
}

#[tokio::test]
async fn cyclic_include() -> Result<(), Box<dyn std::error::Error>> {
    const ASSETS: reinda::Setup  = reinda::assets! {
        #![base_path = "tests/files/cyclic_include"]

        "a.txt": { template },
        "b.txt": { template },
        "c.txt": { template },
    };

    fn assert_correct_error(e: Error) {
        match e {
            Error::CyclicInclude(cycle) => {
                assert!(cycle.contains(&"a.txt".into()));
                assert!(cycle.contains(&"b.txt".into()));
                assert!(cycle.contains(&"c.txt".into()));
            }
            _ => panic!("wrong error: {}", e),
        }
    }

    // Prod
    #[cfg(any(not(debug_assertions), feature = "debug_is_prod"))]
    {
        let e = Assets::new(ASSETS, Config::default()).await.unwrap_err();
        assert_correct_error(e);
    }

    // Dev
    #[cfg(all(debug_assertions, not(feature = "debug_is_prod")))]
    {
        let a = Assets::new(ASSETS, Config::default()).await?;
        assert_correct_error(a.get("a.txt").await.unwrap_err());
        assert_correct_error(a.get("b.txt").await.unwrap_err());
        assert_correct_error(a.get("c.txt").await.unwrap_err());
    }

    Ok(())
}

#[tokio::test]
#[cfg(feature = "hash")]
async fn use_case_web() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;

    const ASSETS: reinda::Setup  = reinda::assets! {
        #![base_path = "tests/files/use_case_web"]

        "index.html": { template },
        "bundle.js": {
            template,
            hash,
            append: "//# sourceMappingURL=/{{: path:bundle.js.map :}}"
        },
        "bundle.js.map": { hash },

        "logo.svg": { hash, dynamic },

        "fonts.css": {
            template,
            serve: false,
        },

        "fonts/comic-sans.woff2": { hash },
    };

    let mut variables = HashMap::new();
    variables.insert("banana".into(), "Yummy food".into());
    let config = Config {
        variables,
        .. Config::default()
    };
    let a = Assets::new(ASSETS, config).await?;



    // Prod
    #[cfg(any(not(debug_assertions), feature = "debug_is_prod"))]
    {
        dbg!(&a);
        assert_eq!(a.asset_ids().count(), 6);
        assert!(a.get("fonts.css").await?.is_none());

        assert_get!(
            a.get("index.html"),
            b"Include fonts: Good font: fonts/comic-sans.u8-1qCm1qg9z.woff2\n\n\
                Link to JS: bundle.KyQ5lDXq4JFo.js\nA variable: Yummy food\n"
        );

        assert_get!(
            a.get("bundle.KyQ5lDXq4JFo.js"),
            b"function doIt() {}\n//# sourceMappingURL=/bundle.zyuDzuDfiJjg.js.map"
        );
        assert_get!(a.get("bundle.zyuDzuDfiJjg.js.map"), b"Mappi McMapFace\n");
        assert_get!(a.get("fonts/comic-sans.u8-1qCm1qg9z.woff2"), b"good stuff\n");
        assert_get!(a.get("logo.0xoy2ercl8KS.svg"), b"Cool logo {{: not a template btw :}}\n");

    }

    // Dev
    #[cfg(all(debug_assertions, not(feature = "debug_is_prod")))]
    {
        // Check info
        let normal_files = &[
            "index.html", "bundle.js", "bundle.js.map", "fonts/comic-sans.woff2",
        ];
        for &name in normal_files {
            let info = a.asset_info(a.lookup(name).unwrap());
            assert_eq!(info.is_served(), true);
            assert_eq!(info.is_dynamic(), false);
        }

        assert_eq!(a.asset_info(a.lookup("logo.svg").unwrap()).is_served(), true);
        assert_eq!(a.asset_info(a.lookup("logo.svg").unwrap()).is_dynamic(), true);

        assert_eq!(a.asset_ids().count(), 6);
        assert!(a.get("fonts.css").await?.is_none());

        // Check contents
        assert_get!(
            a.get("index.html"),
            b"Include fonts: Good font: fonts/comic-sans.woff2\n\n\
                Link to JS: bundle.js\nA variable: Yummy food\n"
        );

        assert_get!(
            a.get("bundle.js"),
            b"function doIt() {}\n//# sourceMappingURL=/bundle.js.map"
        );
        assert_get!(a.get("bundle.js.map"), b"Mappi McMapFace\n");
        assert_get!(a.get("fonts/comic-sans.woff2"), b"good stuff\n");
        assert_get!(a.get("logo.svg"), b"Cool logo {{: not a template btw :}}\n");
    }

    Ok(())
}
