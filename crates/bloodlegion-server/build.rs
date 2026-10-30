use std::fmt::Write as _;
use std::path::Path;

// `sqlx::migrate!()` embeds the migrations at compile time, but cargo does not know it read them:
// without this, adding a migration leaves a stale binary that never applies it.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=data");
    game_item_icons();
}

/// Embeds the item snapshot's icons: `$OUT_DIR/game_item_icons.rs` is a slice of every
/// `data/game-items/icons/<name>.<ext>` as `(name, ext, bytes)`, sorted by name; none when the
/// snapshot has no icons directory.
fn game_item_icons() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dir = Path::new(&manifest).join("data/game-items/icons");
    let mut icons: Vec<_> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("reading {}: {error}", dir.display()),
    };
    icons.sort();
    let mut out = String::from("&[\n");
    for path in icons {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        writeln!(
            out,
            "    ({name:?}, {ext:?}, include_bytes!({:?})),",
            path.display()
        )
        .unwrap();
    }
    out.push(']');
    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(Path::new(&out_dir).join("game_item_icons.rs"), out).unwrap();
}
