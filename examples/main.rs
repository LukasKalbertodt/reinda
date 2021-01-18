const ASSETS: reinda::Setup  = reinda::assets! {
    "assets/index.html": { template },
    "assets/bundle.js": {
        hash,
        append: "//# sourceMappingURL={{: path:bundle.js.map :}}",
    },
    "assets/bundle.js.map": { hash },

    "assets/style.css": { template, serve: false },
};

fn main() {
    println!("{:#?}", ASSETS);
}
