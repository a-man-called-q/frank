//! Port of `apps/frank-gui/src/pages/Integrations.tsx`.

use frank_app::TargetOperation;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::model::PendingKey;
use crate::reducer::target_actions_enabled;
use crate::{Message, Model};

pub fn view(model: &Model) -> Element<'_, Message> {
    let Some(snapshot) = &model.snapshot else {
        return text("Loading Frank…").into();
    };

    let mut list = column![].spacing(12);
    for target in &snapshot.targets {
        let enabled = target_actions_enabled(target) && model.pending.is_none();
        let is_pending_install = matches!(
            &model.pending,
            Some(PendingKey::Target { target_id, operation: TargetOperation::Install })
                if target_id == &target.id
        );
        let is_pending_uninstall = matches!(
            &model.pending,
            Some(PendingKey::Target { target_id, operation: TargetOperation::Uninstall })
                if target_id == &target.id
        );

        let install_label = if is_pending_install {
            "Preparing…"
        } else {
            "Preview install"
        };
        let uninstall_label = if is_pending_uninstall {
            "Preparing…"
        } else {
            "Uninstall"
        };

        let actions = row![
            button(text(install_label)).on_press_maybe(enabled.then_some(
                Message::TargetChangeRequested {
                    target_id: target.id.clone(),
                    operation: TargetOperation::Install,
                }
            )),
            button(text(uninstall_label)).on_press_maybe(enabled.then_some(
                Message::TargetChangeRequested {
                    target_id: target.id.clone(),
                    operation: TargetOperation::Uninstall,
                }
            )),
        ]
        .spacing(8);

        let status = if target.detected {
            "Detected"
        } else {
            "Not detected"
        };
        let unverified = if target.verified {
            ""
        } else {
            " · Unverified"
        };
        let card = column![
            text(target.label.clone()),
            text(format!("{status}{unverified}")),
            text(format!("{} · {}", target.kind, target.source)),
            actions,
        ]
        .spacing(4);
        list = list.push(container(card).padding(12).width(Length::Fill));
    }

    column![text("Integrations").size(20), scrollable(list)]
        .spacing(16)
        .into()
}
