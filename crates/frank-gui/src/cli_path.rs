//! Resolves the `frank` CLI binary the GUI installs hooks to point at, so
//! installed hooks run standalone rather than through the GUI executable.
//!
//! Much simpler than the Tauri version's `resource_dir()/frank-cli/{name}`
//! dance: cargo-packager (M-6) ships `frank` and `frank-gui` as siblings in
//! the same bundle directory (e.g. `Frank.app/Contents/MacOS/` on macOS),
//! so there is no separate "sidecar resource" concept to resolve through.

use std::path::Path;

use frank_app::FrankPaths;

const CLI_NAME: &str = if cfg!(windows) { "frank.exe" } else { "frank" };

pub fn resolve() -> FrankPaths {
    let paths = FrankPaths::from_process();
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .map(|dir| dir.join(CLI_NAME))
        .filter(|path| path.is_file())
        .map(|frank_bin| paths.clone().with_frank_bin(frank_bin))
        .unwrap_or(paths)
}
