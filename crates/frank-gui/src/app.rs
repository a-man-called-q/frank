use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use auto_launch::AutoLaunch;
use frank_app::FrankService;
use frank_gui_core::{Effect, Message, Model};
use iced::{Element, Subscription, Task, window};

use crate::tray;

pub struct State {
    model: Model,
    service: FrankService,
    autostart: AutoLaunch,
    window: Option<window::Id>,
    show_request_path: PathBuf,
    // Held for the process lifetime; never read, dropping it releases the
    // single-instance lock. See `frank_safeio::LockGuard`.
    _lock: frank_safeio::LockGuard,
}

pub fn run(
    hidden: bool,
    service: FrankService,
    autostart: AutoLaunch,
    show_request_path: PathBuf,
    lock: frank_safeio::LockGuard,
) -> iced::Result {
    // `BootFn` requires `Fn`, not `FnOnce` -- iced's trait bound allows a
    // closure that could in principle run more than once, even though in
    // practice the runtime calls it exactly once. `LockGuard` has no
    // sensible "clone", so it is threaded through interior mutability and
    // taken exactly once on the one real call.
    let lock = RefCell::new(Some(lock));

    iced::daemon(
        move || {
            boot(
                hidden,
                service.clone(),
                autostart.clone(),
                show_request_path.clone(),
                lock.borrow_mut().take(),
            )
        },
        update,
        view,
    )
    .title(|_state: &State, _id| "Frank".to_string())
    .subscription(subscription)
    .run()
}

fn window_settings() -> window::Settings {
    let mut settings = window::Settings::default();
    if let Ok((rgba, width, height)) = tray::load_icon_rgba() {
        if let Ok(icon) = window::icon::from_rgba(rgba, width, height) {
            settings.icon = Some(icon);
        }
    }
    settings
}

fn boot(
    hidden: bool,
    service: FrankService,
    autostart: AutoLaunch,
    show_request_path: PathBuf,
    lock: Option<frank_safeio::LockGuard>,
) -> (State, Task<Message>) {
    let mut state = State {
        model: Model::default(),
        service,
        autostart,
        window: None,
        show_request_path,
        _lock: lock.expect("boot() is only ever invoked once by iced's runtime"),
    };

    let refresh = Task::done(Message::RefreshRequested);
    let window_task = if hidden {
        Task::none()
    } else {
        let (id, open) = window::open(window_settings());
        state.window = Some(id);
        open.map(|_id| Message::WindowOpened)
    };

    (state, Task::batch([refresh, window_task]))
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    // `Tick` carries no meaning of its own (see the doc comment on
    // `frank_gui_core::Message::Tick`): the shell intercepts it here to
    // check the show-request handoff file synchronously, translating into
    // a real message *before* `reduce` ever sees it, rather than teaching
    // frank-gui-core about a file it has no business knowing exists.
    let message = if matches!(message, Message::Tick) && state.show_request_path.exists() {
        let _ = frank_safeio::remove_file(&state.show_request_path);
        Message::ShowRequested
    } else {
        message
    };

    let effect = frank_gui_core::reduce(&mut state.model, message);
    interpret(effect, state)
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    frank_gui_core::view(&state.model)
}

fn subscription(state: &State) -> Subscription<Message> {
    let mut subscriptions = vec![
        window::close_requests().map(|_id| Message::CloseRequested),
        runtime_ready_once(),
        tray::events().map(Message::Tray),
        // Always-on, not gated on window visibility like the snapshot poll
        // below: this is exactly how a hidden, tray-only process learns a
        // second launch handed off to it and should show its window.
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
    ];
    if let Some(interval) = frank_gui_core::poll_interval(&state.model) {
        subscriptions.push(iced::time::every(interval).map(|_| Message::RefreshRequested));
    }
    Subscription::batch(subscriptions)
}

fn runtime_ready_once() -> Subscription<Message> {
    Subscription::run(|| futures::stream::once(async { Message::RuntimeReady }))
}

/// Handles every `Effect` the pure reducer can produce, including the ones
/// `frank_gui_core::interpret` deliberately leaves as `Task::none()`
/// because they need a `Platform` implementation or the real
/// `iced::window::Id` -- both of which only this shell owns. Recurses into
/// itself (not `frank_gui_core::interpret`) for `Batch`, since a batch can
/// mix a `Call` with a platform effect like `SetAutostart`.
fn interpret(effect: Effect, state: &mut State) -> Task<Message> {
    match effect {
        Effect::Batch(effects) => {
            Task::batch(effects.into_iter().map(|effect| interpret(effect, state)))
        }
        Effect::OpenWindow => {
            if let Some(id) = state.window {
                window::set_mode(id, window::Mode::Windowed)
            } else {
                let (id, open) = window::open(window_settings());
                state.window = Some(id);
                open.map(|_id| Message::WindowOpened)
            }
        }
        Effect::HideWindow => match state.window {
            Some(id) => window::set_mode(id, window::Mode::Hidden),
            None => Task::none(),
        },
        Effect::PickDirectory => Task::perform(
            async { rfd::AsyncFileDialog::new().pick_folder().await },
            |handle| Message::DirectoryPicked(handle.map(|h| h.path().to_path_buf())),
        ),
        Effect::SetAutostart(enabled) => {
            let result = if enabled {
                state.autostart.enable()
            } else {
                state.autostart.disable()
            }
            .map_err(|e| e.to_string());
            Task::done(Message::AutostartApplied { enabled, result })
        }
        Effect::UpdateTrayStatus(active) => {
            tray::update_status(active.as_deref());
            Task::none()
        }
        Effect::InstallTray => {
            let pack = state.service.pack_or_builtin();
            let active = state.service.active_level().ok().flatten();
            let levels: Vec<String> = pack.levels.keys().cloned().collect();
            tray::install(&levels, active.as_deref());
            Task::none()
        }
        other => frank_gui_core::interpret(other, &state.service),
    }
}
