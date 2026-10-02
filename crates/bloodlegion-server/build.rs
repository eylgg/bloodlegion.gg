// `sqlx::migrate!()` embeds the migrations at compile time, but cargo does not know it read them:
// without this, adding a migration leaves a stale binary that never applies it.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
