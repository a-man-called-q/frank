//! GUI-agnostic model, message, and reducer for the Frank desktop control
//! panel. This crate deliberately has no `iced` dependency (that lands in
//! M-3) and no platform dependency (`tray-icon`, `muda`, `auto-launch` --
//! those live in `crates/frank-gui`, M-4). It exists so the state machine
//! that used to be split across `apps/frank-gui/src/**/*.tsx` and
//! `apps/frank-gui/src-tauri/src/lib.rs` can be unit- and mutation-tested
//! as plain Rust.

mod backend;
mod interpret;
mod message;
mod model;
mod platform;
pub(crate) mod reducer;

pub mod i18n;
pub mod pages;
pub mod view;

pub use backend::Backend;
pub use interpret::interpret;
pub use message::{Message, TrayMessage};
pub use model::{Confirm, Model, Page, PendingKey, PlanRef};
pub use platform::Platform;
pub use reducer::{
    Effect, Request, perform_request, poll_interval, reduce, target_actions_enabled, working_label,
};
pub use view::view;
