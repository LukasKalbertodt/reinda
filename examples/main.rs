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

fn main() {
    println!("{:#?}", ASSETS);
}
