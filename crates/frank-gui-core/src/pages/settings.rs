//! Port of `apps/frank-gui/src/pages/Settings.tsx`.

use iced::Element;
use iced::widget::{checkbox, column, pick_list, text};

use crate::{Message, Model};

pub fn view(model: &Model) -> Element<'_, Message> {
    let Some(snapshot) = &model.snapshot else {
        return text("Loading Frank…").into();
    };

    // Options: the literal "off" plus every level id of the *active* pack.
    let mut options = vec!["off".to_string()];
    if let Some(pack) = snapshot.packs.iter().find(|p| p.active) {
        options.extend(pack.levels.iter().map(|level| level.id.clone()));
    }
    let selected = snapshot
        .settings
        .default_level
        .clone()
        .unwrap_or_else(|| snapshot.default_level.clone());

    let disabled = model.pending.is_some();

    let level_row = column![
        text("Default level"),
        pick_list(options, Some(selected), Message::DefaultLevelSelected),
    ]
    .spacing(4);

    let launch_checkbox = checkbox(snapshot.settings.gui.launch_at_login)
        .label("Launch at login")
        .on_toggle_maybe(
            (!disabled).then_some(Message::LaunchAtLoginToggled as fn(bool) -> Message),
        );

    let tray_checkbox = checkbox(snapshot.settings.gui.close_to_tray)
        .label("Close to tray")
        .on_toggle_maybe((!disabled).then_some(Message::CloseToTrayToggled as fn(bool) -> Message));

    let mut doctor = column![text("Doctor").size(16)].spacing(4);
    for diagnosis in &snapshot.diagnoses {
        let mark = if diagnosis.ok { "✓" } else { "!" };
        doctor = doctor.push(text(format!("{mark} {}", diagnosis.message)));
    }
    for error in &snapshot.target_errors {
        doctor = doctor.push(text(format!("! Target manifest: {error}")));
    }

    column![
        text("Settings").size(20),
        level_row,
        launch_checkbox,
        tray_checkbox,
        doctor,
    ]
    .spacing(16)
    .into()
}
