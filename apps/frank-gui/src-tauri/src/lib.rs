use std::sync::Mutex;

use frank_app::{
    DashboardSnapshot, FrankPaths, FrankService, OperationResult, PackOperation,
    PackOperationResult, PackPlanPreview, PlanPreview, TargetOperation, UserSettings,
    UserSettingsPatch,
};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

struct AppState {
    service: Mutex<FrankService>,
}

fn service<'a>(
    state: &'a State<'a, AppState>,
) -> Result<std::sync::MutexGuard<'a, FrankService>, String> {
    state
        .service
        .lock()
        .map_err(|_| "Frank service lock poisoned".to_string())
}

#[tauri::command]
fn snapshot(state: State<'_, AppState>) -> Result<DashboardSnapshot, String> {
    service(&state)?.snapshot().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_active_level(
    level: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    service(&state)?
        .set_active_level(level.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    patch: UserSettingsPatch,
    state: State<'_, AppState>,
) -> Result<UserSettings, String> {
    let launch_at_login = patch.launch_at_login;
    let settings = service(&state)?
        .update_settings(patch)
        .map_err(|e| e.to_string())?;
    if let Some(enabled) = launch_at_login {
        let manager = app.autolaunch();
        if enabled {
            manager
                .enable()
                .map_err(|e| format!("settings saved, but autostart could not be enabled: {e}"))?;
        } else {
            manager
                .disable()
                .map_err(|e| format!("settings saved, but autostart could not be disabled: {e}"))?;
        }
    }
    Ok(settings)
}

#[tauri::command]
fn prepare_target_change(
    target_id: String,
    operation: TargetOperation,
    state: State<'_, AppState>,
) -> Result<PlanPreview, String> {
    service(&state)?
        .prepare_target_change(&target_id, operation)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_prepared_plan(
    plan_id: String,
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    service(&state)?
        .apply_prepared_plan(&plan_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn prepare_pack_change(
    operation: PackOperation,
    state: State<'_, AppState>,
) -> Result<PackPlanPreview, String> {
    service(&state)?
        .prepare_pack_change(operation)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn apply_prepared_pack(
    plan_id: String,
    state: State<'_, AppState>,
) -> Result<PackOperationResult, String> {
    service(&state)?
        .apply_prepared_pack(&plan_id)
        .map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch forwards its argv to the first process and brings
        // the existing window forward. This keeps autostart/relaunch flows
        // tray-first instead of creating two writers against the same plans.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--hidden"])
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            set_active_level,
            update_settings,
            prepare_target_change,
            apply_prepared_plan,
            prepare_pack_change,
            apply_prepared_pack
        ])
        .setup(|app| {
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            let paths = FrankPaths::from_process();
            let paths = app
                .path()
                .resource_dir()
                .ok()
                // The CLI resource is deliberately nested. Tauri's external
                // sidecar staging uses the basename at target/debug/frank;
                // keeping the bundled CLI under frank-cli/ prevents a GUI
                // build script from overwriting Cargo's frank test binary.
                // Windows selects the PE resource with its `.exe` suffix.
                .map(|dir| {
                    let name = if cfg!(windows) { "frank.exe" } else { "frank" };
                    dir.join("frank-cli").join(name)
                })
                .filter(|path| path.is_file())
                .or_else(|| {
                    // `tauri dev` has no packaged resource directory yet;
                    // use the sibling CLI Cargo just built in target/debug.
                    let name = if cfg!(windows) { "frank.exe" } else { "frank" };
                    std::env::current_exe()
                        .ok()
                        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
                        .filter(|path| path.is_file())
                })
                .map(|path| paths.clone().with_frank_bin(path))
                .unwrap_or(paths);
            app.manage(AppState {
                service: Mutex::new(FrankService::new(paths)),
            });
            let (active_level, levels) = app
                .state::<AppState>()
                .service
                .lock()
                .ok()
                .map(|service| {
                    let active = service.active_level().ok().flatten();
                    let levels = service
                        .current_pack()
                        .ok()
                        .map(|pack| pack.levels.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default();
                    (active, levels)
                })
                .unwrap_or_default();
            let status_text = active_level
                .as_deref()
                .map(|level| format!("Status: active ({level})"))
                .unwrap_or_else(|| "Status: off".to_string());
            let status = MenuItemBuilder::with_id("status", status_text)
                .enabled(false)
                .build(app)?;
            let open = MenuItemBuilder::with_id("open", "Open Frank").build(app)?;
            let off = MenuItemBuilder::with_id("level:off", "Turn off").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Frank").build(app)?;
            let level_items = levels
                .iter()
                .map(|level| {
                    MenuItemBuilder::with_id(
                        format!("level:{level}"),
                        format!("Use level: {level}"),
                    )
                    .build(app)
                })
                .collect::<tauri::Result<Vec<_>>>()?;
            let mut menu_builder = MenuBuilder::new(app).item(&status).item(&off);
            for item in &level_items {
                menu_builder = menu_builder.item(item);
            }
            let menu = menu_builder.item(&open).item(&quit).build()?;
            tauri::tray::TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_main(app),
                    id if id.starts_with("level:") => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(service) = state.service.lock() {
                                let _ = service.set_active_level(
                                    id.strip_prefix("level:").filter(|l| *l != "off"),
                                );
                            }
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            if std::env::args().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let keep_in_tray = window
                    .state::<AppState>()
                    .service
                    .lock()
                    .ok()
                    .and_then(|s| s.settings().ok())
                    .map(|s| s.gui.close_to_tray)
                    .unwrap_or(true);
                if keep_in_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Frank GUI");
}
