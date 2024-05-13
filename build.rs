fn main() {
    cfg_aliases::cfg_aliases! {
        prod_mode: { any(not(debug_assertions), feature = "always-embed") },
        dev_mode: { not(prod_mode) },
    }
}
