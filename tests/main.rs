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


    #[cfg(all(debug_assertions, not(feature = "debug_is_prod")))]
    {
        let a = Assets::new(ASSETS, Config::default()).await?;
        assert_correct_error(a.get("a.txt").await.unwrap_err());
        assert_correct_error(a.get("b.txt").await.unwrap_err());
        assert_correct_error(a.get("c.txt").await.unwrap_err());
    }

    Ok(())
}
