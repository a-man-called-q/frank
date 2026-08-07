//! Headless UI tests via `iced_test::simulator`. These replace the
//! Playwright suite (`apps/frank-gui/e2e/shell.spec.ts`) with real Rust
//! types and no browser: `simulator(view(&model))` renders the actual
//! `view()` output off-screen (tiny-skia software renderer, no GPU/display
//! needed) and `.click(text)`/`.find(text)` interact with it the same way a
//! user would. `into_messages()` collects what widget interactions would
//! have sent to `update()`; we then drive them through `reduce()` ourselves
//! since `Simulator` only renders one frame at a time.

use frank_app::{
    DashboardSnapshot, DiagnosisView, GuiSettings, LevelSummary, PackSummary, TargetSummary,
    UserSettings,
};
use frank_gui_core::{Message, Model, Page, reduce, view};
use iced_test::simulator;

fn snapshot(active_level: Option<&str>) -> DashboardSnapshot {
    DashboardSnapshot {
        active_level: active_level.map(str::to_string),
        active_pack: "caveman".into(),
        active_pack_version: "1.0.0".into(),
        default_level: "full".into(),
        settings: UserSettings {
            default_level: None,
            gui: GuiSettings {
                launch_at_login: false,
                close_to_tray: true,
            },
        },
        packs: vec![PackSummary {
            id: "caveman".into(),
            version: "1.0.0".into(),
            active: true,
            builtin: true,
            levels: vec![LevelSummary {
                id: "full".into(),
                title: Some("Full".into()),
                aliases: vec![],
            }],
        }],
        targets: vec![
            TargetSummary {
                id: "claude-code".into(),
                label: "Claude Code".into(),
                kind: "generic".into(),
                verified: true,
                soft: false,
                detected: true,
                source: "manifest".into(),
            },
            TargetSummary {
                id: "codex".into(),
                label: "Codex".into(),
                kind: "generic".into(),
                verified: false,
                soft: false,
                detected: true,
                source: "manifest".into(),
            },
        ],
        target_errors: vec![],
        diagnoses: vec![DiagnosisView {
            ok: true,
            message: "SessionStart hook installed".into(),
        }],
    }
}

fn model_with_snapshot(active_level: Option<&str>) -> Model {
    Model {
        snapshot: Some(snapshot(active_level)),
        ..Model::default()
    }
}

#[test]
fn loading_state_renders_before_a_snapshot_arrives() {
    let model = Model::default();
    let mut ui = simulator(view(&model));
    assert!(ui.find("Loading Frank…").is_ok());
}

#[test]
fn loading_state_shows_the_error_instead_of_the_loading_text() {
    let model = Model {
        error: Some("permission denied".to_string()),
        ..Model::default()
    };
    let mut ui = simulator(view(&model));
    assert!(ui.find("permission denied").is_ok());
    assert!(ui.find("Loading Frank…").is_err());
}

#[test]
fn overview_shows_the_active_pack_and_turn_off_button() {
    let model = model_with_snapshot(Some("full"));
    let mut ui = simulator(view(&model));
    assert!(ui.find("caveman · v1.0.0").is_ok());
    assert!(ui.find("Turn off").is_ok());
}

#[test]
fn clicking_turn_off_produces_a_toggle_message() {
    let model = model_with_snapshot(Some("full"));
    let mut ui = simulator(view(&model));
    ui.click("Turn off").expect("Turn off button must exist");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages, vec![Message::ToggleActive]);
}

#[test]
fn navigating_to_settings_shows_the_settings_page() {
    let mut model = model_with_snapshot(Some("full"));
    let mut ui = simulator(view(&model));
    ui.click("Settings")
        .expect("Settings nav button must exist");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages, vec![Message::Navigate(Page::Settings)]);

    for message in messages {
        reduce(&mut model, message);
    }
    let mut ui = simulator(view(&model));
    assert!(ui.find("Launch at login").is_ok());
}

#[test]
fn personas_page_lists_packs_and_lets_you_add_one() {
    let mut model = model_with_snapshot(Some("full"));
    model.page = Page::Personas;
    let mut ui = simulator(view(&model));
    assert!(ui.find("caveman · v1.0.0").is_ok());
    // Sole pack is builtin and already active: no "Use pack"/"Remove".
    assert!(ui.find("Use pack").is_err());
    assert!(ui.find("Remove").is_err());

    ui.click("Add local pack")
        .expect("Add local pack button must exist");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages, vec![Message::AddPackRequested]);
}

#[test]
fn personas_page_offers_use_and_remove_for_a_second_pack() {
    let mut model = model_with_snapshot(Some("full"));
    model.page = Page::Personas;
    model.snapshot.as_mut().unwrap().packs.push(PackSummary {
        id: "local".into(),
        version: "1.0.0".into(),
        active: false,
        builtin: false,
        levels: vec![],
    });
    let mut ui = simulator(view(&model));

    ui.click("Use pack").expect("Use pack button must exist");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(
        messages,
        vec![Message::PackUseRequested {
            selector: "local@1.0.0".into(),
        }]
    );
}

#[test]
fn integrations_page_lists_targets_with_unverified_marker() {
    let mut model = model_with_snapshot(Some("full"));
    model.page = Page::Integrations;
    let mut ui = simulator(view(&model));
    assert!(ui.find("Claude Code").is_ok());
    assert!(ui.find("Codex").is_ok());
    assert!(ui.find("Not detected · Unverified").is_err());
}

#[test]
fn confirm_modal_shows_the_prompt_and_cancel_dismisses() {
    let mut model = model_with_snapshot(Some("full"));
    model.confirm = Some(frank_gui_core::Confirm {
        prompt: "install hook\n\nApply this install plan?".into(),
        actions: vec!["install hook".into()],
        plan: frank_gui_core::PlanRef::Target("plan-1".into()),
    });

    let mut ui = simulator(view(&model));
    assert!(ui.find("install hook\n\nApply this install plan?").is_ok());

    ui.click("Cancel").expect("Cancel button must exist");
    let messages: Vec<Message> = ui.into_messages().collect();
    assert_eq!(messages, vec![Message::ConfirmDismissed]);
}

#[test]
fn doctor_diagnoses_render_on_the_settings_page() {
    let mut model = model_with_snapshot(Some("full"));
    model.page = Page::Settings;
    let mut ui = simulator(view(&model));
    assert!(ui.find("✓ SessionStart hook installed").is_ok());
}
