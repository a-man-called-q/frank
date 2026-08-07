//! Port of `apps/frank-gui/src/pages/Personas.tsx`.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::model::PendingKey;
use crate::{Message, Model};

pub fn view(model: &Model) -> Element<'_, Message> {
    let Some(snapshot) = &model.snapshot else {
        return text("Loading Frank…").into();
    };

    let add_label = if matches!(model.pending, Some(PendingKey::PackAdd)) {
        "Adding…"
    } else {
        "Add local pack"
    };
    let header = row![
        text("Personas").size(20),
        button(text(add_label))
            .on_press_maybe(model.pending.is_none().then_some(Message::AddPackRequested)),
    ]
    .spacing(12);

    let mut list = column![].spacing(12);
    for pack in &snapshot.packs {
        let selector = format!("{}@{}", pack.id, pack.version);
        let title = format!("{} · v{}", pack.id, pack.version);
        let level_ids = pack
            .levels
            .iter()
            .map(|level| level.id.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let kind = if pack.builtin {
            "Built-in"
        } else {
            "Installed"
        };

        let mut actions = row![].spacing(8);
        if !pack.active {
            let pending = matches!(
                &model.pending,
                Some(PendingKey::PackUse { selector: s }) if *s == selector
            );
            actions = actions.push(
                button(text(if pending { "Preparing…" } else { "Use pack" })).on_press_maybe(
                    model
                        .pending
                        .is_none()
                        .then_some(Message::PackUseRequested {
                            selector: selector.clone(),
                        }),
                ),
            );
        }
        if !pack.builtin {
            let pending = matches!(
                &model.pending,
                Some(PendingKey::PackRemove { selector: s }) if *s == selector
            );
            actions = actions.push(
                button(text(if pending { "Preparing…" } else { "Remove" })).on_press_maybe(
                    model
                        .pending
                        .is_none()
                        .then_some(Message::PackRemoveRequested {
                            selector: selector.clone(),
                        }),
                ),
            );
        }

        let card = column![
            text(title),
            text(format!(
                "{kind}{} · {} levels · {level_ids}",
                if pack.active { " · Active" } else { "" },
                pack.levels.len()
            )),
            actions,
        ]
        .spacing(4);
        list = list.push(container(card).padding(12).width(Length::Fill));
    }

    column![header, scrollable(list)].spacing(16).into()
}
