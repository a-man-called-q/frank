use iced::Task;

use crate::backend::Backend;
use crate::message::Message;
use crate::reducer::{Effect, perform_request};

/// Turns an `Effect` into real async work. The `Call` case is the only one
/// this crate can fully own: it needs nothing but a `Backend`, so it is
/// generic over any implementation (real `FrankService` in `crates/frank-gui`,
/// `RecordingBackend`/etc. in tests).
///
/// `PickDirectory`, `SetAutostart`, `OpenWindow`, `HideWindow`,
/// `UpdateTrayStatus`, and `InstallTray` need a `Platform` implementation
/// and/or the real `iced::window::Id`, neither of which this crate owns by
/// design (see the plan's M-2/M-3/M-4 split: platform crates like
/// `tray-icon`/`muda`/`auto-launch` stay out of the coverage-gated crate).
/// They resolve to `Task::none()` here; `crates/frank-gui` (M-4) wraps this
/// function and handles them for real.
///
/// Not unit-tested here: `Task<Message>` is opaque by design (iced only
/// exposes execution through its own runtime), so verifying that a `Call`
/// effect's future actually resolves to the right `Message` needs a real
/// event loop. That happens in M-4's `native_smoke.sh` integration test
/// against the real `crates/frank-gui` binary. `perform_request` (the
/// synchronous core this wraps) is unit- and integration-tested directly in
/// `reducer.rs` and `tests/backend_contract.rs`; `interpret`'s own job is
/// just choosing *which* iced primitive (`Task::perform`/`none`/`batch`,
/// `iced::exit()`) to hand a `Request`/`Effect` to, which is what M-4's
/// smoke test exercises end-to-end.
pub fn interpret<B>(effect: Effect, backend: &B) -> Task<Message>
where
    B: Backend + Clone + Send + Sync + 'static,
{
    match effect {
        Effect::None => Task::none(),
        Effect::Batch(effects) => {
            Task::batch(effects.into_iter().map(|effect| interpret(effect, backend)))
        }
        Effect::Call(request) => {
            let backend = backend.clone();
            Task::perform(
                async move { perform_request(&backend, request) },
                |message| message,
            )
        }
        Effect::Quit => iced::exit(),
        Effect::PickDirectory
        | Effect::SetAutostart(_)
        | Effect::OpenWindow
        | Effect::HideWindow
        | Effect::UpdateTrayStatus(_)
        | Effect::InstallTray => Task::none(),
    }
}
