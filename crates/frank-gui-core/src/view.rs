//! Top-level shell: sidebar nav + page content + error banner + confirm
//! modal overlay. Port of `apps/frank-gui/src/App.tsx`'s JSX. Deliberately
//! not chasing pixel parity with `styles.css` -- see the plan's "Styling"
//! note: this is a redesign, not a port, and the lowest-risk part of the
//! migration.

use iced::widget::{button, center, column, container, opaque, row, stack, text};
use iced::{Element, Length};

use crate::model::Page;
use crate::pages;
use crate::{Message, Model};

pub fn view(model: &Model) -> Element<'_, Message> {
    let Some(_) = &model.snapshot else {
        let message = model
            .error
            .clone()
            .unwrap_or_else(|| "Loading Frank…".to_string());
        return column![text("Frank").size(24), text(message)]
            .spacing(8)
            .padding(20)
            .into();
    };

    let nav = column![
        nav_button("Overview", Page::Overview, model.page),
        nav_button("Personas", Page::Personas, model.page),
        nav_button("Integrations", Page::Integrations, model.page),
        nav_button("Settings", Page::Settings, model.page),
    ]
    .spacing(4)
    .width(Length::Fixed(180.0));

    let page = match model.page {
        Page::Overview => pages::overview::view(model),
        Page::Personas => pages::personas::view(model),
        Page::Integrations => pages::integrations::view(model),
        Page::Settings => pages::settings::view(model),
    };

    let mut content = column![].spacing(12).width(Length::Fill);
    if let Some(error) = &model.error {
        content = content.push(text(error.clone()));
    }
    content = content.push(page);

    let base: Element<'_, Message> = row![nav, content].spacing(24).padding(20).into();

    match &model.confirm {
        Some(confirm) => {
            let dialog = container(
                column![
                    text(confirm.prompt.clone()),
                    row![
                        button(text("Cancel")).on_press(Message::ConfirmDismissed),
                        button(text("Confirm")).on_press(Message::ConfirmAccepted),
                    ]
                    .spacing(8),
                ]
                .spacing(12),
            )
            .padding(20)
            .width(Length::Fixed(420.0));

            stack![base, opaque(center(opaque(dialog)))].into()
        }
        None => base,
    }
}

fn nav_button(label: &str, target: Page, current: Page) -> Element<'_, Message> {
    let label = if target == current {
        format!("▸ {label}")
    } else {
        label.to_string()
    };
    button(text(label))
        .on_press(Message::Navigate(target))
        .into()
}
