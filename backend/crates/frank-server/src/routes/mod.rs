//! HTTP surface. One module per resource; `build_router` in the crate root is
//! the only place they are wired together.

pub(crate) mod artifacts;
pub(crate) mod browse;
pub(crate) mod commands;
pub(crate) mod devices;
pub(crate) mod diagnostics;
pub(crate) mod events;
pub(crate) mod pair;
pub(crate) mod terminals;
