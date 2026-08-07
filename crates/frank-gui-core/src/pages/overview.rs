//! Port of `apps/frank-gui/src/pages/Overview.tsx`.

use iced::widget::{button, column, row, text};
use iced::{Element, Length};

use crate::model::PendingKey;
use crate::{Message, Model};

pub fn view(model: &Model) -> Element<'_, Message> {
    let Some(snapshot) = &model.snapshot else {
        return text("Loading Frank…").into();
    };

    let is_pending = matches!(model.pending, Some(PendingKey::Toggle));
    let toggle_label = if is_pending {
        "Working…"
    } else if snapshot.active_level.is_some() {
        "Turn off"
    } else {
        "Turn on"
    };

    let status_line = match &snapshot.active_level {
        Some(level) => format!("Level {level} is reinforcing every turn."),
        None => "Frank is ready when you are.".to_string(),
    };

    let hero = column![
        text(format!(
            "{} · v{}",
            snapshot.active_pack, snapshot.active_pack_version
        ))
        .size(20),
        text(status_line),
        button(text(toggle_label))
            .on_press_maybe(model.pending.is_none().then_some(Message::ToggleActive)),
    ]
    .spacing(8);

    let detected = snapshot.targets.iter().filter(|t| t.detected).count();
    let verified = snapshot
        .targets
        .iter()
        .filter(|t| t.verified && t.detected)
        .count();

    let stats = row![
        column![text("Default level"), text(snapshot.default_level.clone())].spacing(4),
        column![
            text("Integrations"),
            text(format!("{detected} detected · {verified} verified ready"))
        ]
        .spacing(4),
    ]
    .spacing(24);

    column![hero, stats].spacing(16).width(Length::Fill).into()
}
