mod app;
mod cli_path;
mod tray;

use std::ffi::OsStr;

use auto_launch::AutoLaunchBuilder;
use frank_app::FrankService;

const LOCK_NAME: &str = "gui.lock";
const SHOW_REQUEST_NAME: &str = "gui.show-request";

/// macOS `AutoLaunchBuilder::set_app_path` needs the `.app` bundle path, not
/// `current_exe()` -- inside a bundle `current_exe()` resolves to
/// `Frank.app/Contents/MacOS/frank-gui`, and LSSharedFileList's login-item
/// registration expects the bundle itself. A naive port that passed
/// `current_exe()` straight through would make "Launch at login" silently
/// no-op. Walk up to the nearest `*.app` ancestor; outside a bundle (a bare
/// dev build) there is none, and `current_exe()` is used as-is.
#[cfg(target_os = "macos")]
fn autostart_target_path(exe: &std::path::Path) -> std::path::PathBuf {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|ext| ext == "app"))
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| exe.to_path_buf())
}

#[cfg(not(target_os = "macos"))]
fn autostart_target_path(exe: &std::path::Path) -> std::path::PathBuf {
    exe.to_path_buf()
}

fn main() {
    let hidden = std::env::args().any(|arg| arg == "--hidden");

    let paths = cli_path::resolve();
    let service = FrankService::new(paths.clone());

    let exe = std::env::current_exe().unwrap_or_else(|_| "frank-gui".into());
    let autostart = AutoLaunchBuilder::new()
        .set_app_name("Frank")
        .set_app_path(&autostart_target_path(&exe).to_string_lossy())
        .set_args(&["--hidden"])
        .build()
        .expect("autostart handle construction does not perform IO and cannot fail here");

    let lock_result = frank_safeio::try_lock_exclusive(&paths.data_root, OsStr::new(LOCK_NAME));
    match lock_result {
        Ok(Some(lock)) => {
            let show_request_path = paths.data_root.join(SHOW_REQUEST_NAME);
            app::run(hidden, service, autostart, show_request_path, lock)
                .expect("error while running Frank GUI");
        }
        Ok(None) => {
            // Another instance already holds the lock: hand off and exit
            // without ever touching iced/wgpu/the tray. Argv/cwd are not
            // forwarded -- the Tauri version ignored both too, so nothing
            // downstream ever depended on them.
            let show_request_path = paths.data_root.join(SHOW_REQUEST_NAME);
            let _ = frank_safeio::write_text_atomic(&show_request_path, "show", 16);
        }
        Err(error) => {
            eprintln!("frank-gui: could not acquire the single-instance lock: {error}");
            std::process::exit(1);
        }
    }
}
