// rust-i18n embeds the translations at compile time but does not tell cargo which files they came from.
fn main() {
    println!("cargo:rerun-if-changed=locales");
}
