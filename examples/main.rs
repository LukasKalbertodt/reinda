const ASSETS: reinda::Setup  = reinda::assets! {
    "index.html": { template },
    "bundle.js": {
        hash,
        append: "//# sourceMappingURL={{: path:bundle.js.map :}}",
    },
    "bundle.js.map": { hash },

    "fonts/cyrillic-400.woff2": { hash },
    "fonts/cyrillic-700.woff2": { hash },

    "fonts.css": { template, serve: false },
};

fn main() {
    println!("{:#?}", ASSETS);
}
