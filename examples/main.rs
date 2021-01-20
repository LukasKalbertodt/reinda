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
    let assets = Assets::new(ASSETS, Config::default()).await?;

    for path in ASSETS.assets.iter().map(|a| a.path) {
        println!("### {}", path);

        // match assets.load_raw(path).await? {
        //     None => println!("doesn't exist"),
        //     Some(raw) => {
        //         println!("{:?}", raw.unresolved_fragments);
        //         println!("--------------\n{}-----------", String::from_utf8_lossy(&raw.content));
        //     }
        // }

        match assets.load_dynamic(path).await? {
            None => println!("doesn't exist"),
            Some(bytes) => println!("{}", String::from_utf8_lossy(&bytes)),
        }

        println!();
    }

    Ok(())
}
