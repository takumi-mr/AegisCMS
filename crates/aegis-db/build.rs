//! Rebuild when a migration file changes (`sqlx::migrate!` embeds them at compile time).

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
