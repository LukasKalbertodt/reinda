const ASSETS: reinda::Setup  = reinda::assets! {
    serve: {
        "index.html": { template },
        "bundle.js": {
            hash,
            append: "//# sourceMappingURL={{: path:bundle.js.map :}}",
        },
        "bundle.js.map": { hash },

        "fonts/cyrillic-400.woff2": { hash },
        "fonts/cyrillic-700.woff2": { hash },
    },
    includes: {
        "fonts.css": { template },
    },
};

fn main() {
    println!("{:#?}", ASSETS);
}
