//! Tray icon + menu, backed by `tray-icon`/`muda`.
//!
//! Two hard constraints, both confirmed empirically by the M-0 spike (see
//! the plan), not just read off `tray-icon`'s docs:
//!
//! 1. **Main-thread affinity.** `TrayIcon`/`MenuItem` wrap `Rc<RefCell<..>>`
//!    internally — they are `!Send + !Sync` and will not compile inside a
//!    plain `static`. They live in `thread_local!` storage instead. iced's
//!    `update()` is confirmed (by the spike's own logging) to always run on
//!    the same thread that installs the tray, so reading/writing this
//!    thread-local from `install`/`update_status`/inside `update()` is safe.
//! 2. **The `Subscription::run` closure's *setup* runs on the main thread,
//!    but the `Stream` it returns is polled by iced's executor on a
//!    *different* thread.** Reading the thread-local item-id table from
//!    inside a `.map()` closure on that stream silently saw it empty. The
//!    fix, used here: the subscription forwards only the raw `MenuId` as a
//!    `String` (`Send`-safe; `MenuId`/`MenuItem` are not) and all matching
//!    against stored ids happens in `update()`.

use std::cell::RefCell;

use frank_gui_core::TrayMessage;
use futures::StreamExt;
use futures::channel::mpsc;
use iced::Subscription;
use muda::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static STATUS_ITEM: RefCell<Option<MenuItem>> = const { RefCell::new(None) };
    static OPEN_ID: RefCell<Option<MenuId>> = const { RefCell::new(None) };
    static QUIT_ID: RefCell<Option<MenuId>> = const { RefCell::new(None) };
    static LEVEL_IDS: RefCell<Vec<(MenuId, Option<String>)>> = const { RefCell::new(Vec::new()) };
}

fn status_text(active: Option<&str>) -> String {
    match active {
        Some(level) => format!("Status: active ({level})"),
        None => "Status: off".to_string(),
    }
}

/// Builds the tray icon and its menu: a disabled status line, "Turn off",
/// one "Use level: {id}" item per pack level, "Open Frank", "Quit Frank" --
/// the same structure the Tauri implementation used. Must run on the main
/// thread, after the event loop has pumped at least once (see the module
/// doc comment); the shell only calls this from `Effect::InstallTray`,
/// which is triggered by `Message::RuntimeReady`.
pub fn install(levels: &[String], active: Option<&str>) {
    let already_installed = TRAY.with_borrow(|tray| tray.is_some());
    if already_installed {
        return;
    }

    let menu = Menu::new();
    let status = MenuItem::new(status_text(active), false, None);
    let off = MenuItem::new("Turn off", true, None);
    let open = MenuItem::new("Open Frank", true, None);
    let quit = MenuItem::new("Quit Frank", true, None);

    let _ = menu.append(&status);
    let _ = menu.append(&off);

    let mut level_ids = Vec::with_capacity(levels.len());
    for level in levels {
        let item = MenuItem::new(format!("Use level: {level}"), true, None);
        let _ = menu.append(&item);
        level_ids.push((item.id().clone(), Some(level.clone())));
    }
    level_ids.push((off.id().clone(), None));

    let _ = menu.append(&open);
    let _ = menu.append(&quit);

    let icon = match load_icon() {
        Ok(icon) => Some(icon),
        Err(error) => {
            eprintln!("frank-gui: tray icon failed to load, using a blank icon: {error}");
            None
        }
    };

    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Frank");
    if let Some(icon) = icon {
        builder = builder.with_icon(icon);
    }

    match builder.build() {
        Ok(tray) => {
            OPEN_ID.with_borrow_mut(|id| *id = Some(open.id().clone()));
            QUIT_ID.with_borrow_mut(|id| *id = Some(quit.id().clone()));
            LEVEL_IDS.with_borrow_mut(|ids| *ids = level_ids);
            STATUS_ITEM.with_borrow_mut(|item| *item = Some(status));
            TRAY.with_borrow_mut(|slot| *slot = Some(tray));
        }
        Err(error) => {
            eprintln!("frank-gui: failed to install tray icon: {error}");
        }
    }
}

/// Live-updates the status line's text without rebuilding the menu. This is
/// the one piece of the tray the plan calls out as worth fixing beyond
/// parity: the Tauri version snapshotted this text once at startup and
/// never touched it again, which is the entire reason the GUI polled every
/// 2 seconds even while hidden in a tray nobody was looking at.
pub fn update_status(active: Option<&str>) {
    STATUS_ITEM.with_borrow(|item| {
        if let Some(item) = item {
            item.set_text(status_text(active));
        }
    });
}

fn load_icon() -> Result<Icon, String> {
    // A small solid-color fallback keeps the tray non-blank even if the
    // packaged asset is ever missing; `xtask`/packaging should still ship a
    // real multi-resolution icon (see the plan's M-6 packaging notes).
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for pixel in rgba.chunks_mut(4) {
        pixel[0] = 217; // R
        pixel[1] = 119; // G
        pixel[2] = 6; // B -- amber, matches Frank's theme
        pixel[3] = 255; // A
    }
    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| e.to_string())
}

/// Subscription that forwards raw tray menu-click ids into `update()`. Does
/// no matching itself -- see the module doc comment for why.
pub fn events() -> Subscription<TrayMessage> {
    Subscription::run(|| {
        let (tx, rx) = mpsc::unbounded();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = tx.unbounded_send(event.id().0.clone());
        }));
        // `filter_map`, not `map`: an id that matches nothing we built (a
        // stale event, or a click racing tray teardown) must be dropped
        // outright, not folded into `SetLevel(None)` -- that variant is a
        // real match (the "Turn off" item) and must stay distinguishable
        // from "no match at all".
        rx.filter_map(|raw_id| futures::future::ready(resolve(raw_id)))
    })
}

fn resolve(raw_id: String) -> Option<TrayMessage> {
    let is_open = OPEN_ID.with_borrow(|id| id.as_ref().is_some_and(|id| id.0 == raw_id));
    if is_open {
        return Some(TrayMessage::Open);
    }
    let is_quit = QUIT_ID.with_borrow(|id| id.as_ref().is_some_and(|id| id.0 == raw_id));
    if is_quit {
        return Some(TrayMessage::Quit);
    }
    LEVEL_IDS.with_borrow(|ids| {
        ids.iter()
            .find(|(id, _)| id.0 == raw_id)
            .map(|(_, level)| TrayMessage::SetLevel(level.clone()))
    })
}
