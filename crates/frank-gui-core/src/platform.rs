/// The seam that keeps `tray-icon`/`muda`/`auto-launch` (all main-thread- or
/// event-loop-affine, per the M-0 spike) out of this crate entirely, so
/// `frank-gui-core` stays plain, portable Rust that `cargo test` and
/// `cargo mutants` can exercise without a display or an event loop.
///
/// Implemented for real in `crates/frank-gui` (M-4); implemented by
/// `FakePlatform` in tests here.
pub trait Platform {
    fn install_tray(&self, levels: &[String], active: Option<&str>) -> Result<(), String>;
    fn update_tray_status(&self, active: Option<&str>);
    fn set_autostart(&self, enabled: bool) -> Result<(), String>;
}
