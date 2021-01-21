use std::collections::HashMap;

use reinda::{Assets, Config};

const ASSETS: reinda::Setup  = reinda::assets! {
    #![base_path = "examples/assets"]

    "index.html": { template },
    "bundle.js": {
        hash,
        append: "//# sourceMappingURL={{: path:bundle.js.map :}}",
    },
    "bundle.js.map": { hash },

    "style.css": { template, serve: false },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut variables = HashMap::new();
    variables.insert("name".into(), "peter".into());
    let config = Config {
        variables,
        .. Config::default()
    };
    let assets = Assets::new(ASSETS, config).await?;

    for asset_id in assets.asset_ids() {
        let info = assets.asset_info(asset_id).unwrap();
        println!("########## {}", info.original_path());
        println!("# > public_path: {}", info.public_path());
        println!("# > serve: {}", info.is_served());
        println!("# > dynamic: {}", info.is_dynamic());

        match assets.get(info.public_path()).await? {
            None => println!("# > doesn't exist!!!"),
            Some(bytes) => println!("{}", String::from_utf8_lossy(&bytes)),
        }

        println!();
    }

    Ok(())
}
