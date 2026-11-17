//! The home page: where the sync stands, and the other devices.

use std::time::SystemTime;

use weft_cli::hex;
use windows_reactor::*;

use super::app::{Message, Weft};
use super::widgets::{
    body, button, card, column, danger_button, label, primary_button, secondary, status_mark,
};
use super::words;
use crate::i18n::{is_french, t};
use crate::worker::{Device, PairingKind};

fn status(weft: &Weft) -> View {
    let (text, live) = words::status(&weft.snapshot);
    let line = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((status_mark(live), body(text)));
    let mut views = vec![line];
    if let Some((activity, _)) = words::activity(weft.snapshot.last_activity, SystemTime::now()) {
        views.push(secondary(activity).into());
    }
    column(4.0, views)
}

fn added_text(kind: PairingKind, name: Option<&str>) -> String {
    match kind {
        PairingKind::Joined => match (name, is_french()) {
            (Some(name), false) => format!("Paired with {name}"),
            (Some(name), true) => format!("Associé à {name}"),
            (None, _) => t("Paired with a new device", "Associé à un nouvel appareil").to_owned(),
        },
        PairingKind::Invited | PairingKind::JoinedOther => match (name, is_french()) {
            (Some(name), false) => format!("Joined {name}'s group"),
            (Some(name), true) => format!("Ajouté au groupe de {name}"),
            (None, _) => t("Joined the group", "Ajouté au groupe").to_owned(),
        },
    }
}

/// The notice that a pairing just added a device or joined a group, by name
/// once it is known.
fn added(weft: &Weft, context: &ViewContext<Weft>) -> Option<View> {
    let (device, kind) = weft.added?;
    let name = weft
        .snapshot
        .devices
        .iter()
        .find(|known| known.id == device)
        .and_then(|known| known.name.as_deref());
    let message = added_text(kind, name);
    Some(
        InfoBar::new()
            .severity(InfoBarSeverity::Success)
            .is_open(true)
            .is_closable(true)
            .message(message)
            .on_closed(context.message(Message::DismissAdded))
            .into(),
    )
}

/// A device of the list: its name opens its details, the bin removes it.
fn device_row(weft: &Weft, device: &Device, context: &ViewContext<Weft>) -> View {
    let name = words::device_name(device);
    let remove_label = if is_french() {
        format!("Retirer {name}")
    } else {
        format!("Remove {name}")
    };
    // The short id, after the battery level once the device sent it.
    let mut caption = words::device_id(device);
    let battery = weft.snapshot.peer(&device.id).battery;
    if weft.snapshot.features.battery
        && let Some((figure, _)) = battery.as_ref().and_then(words::battery)
    {
        caption = format!("{figure} · {caption}");
    }
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                Button::new()
                    .grid_column(0)
                    .style(ButtonStyle::Subtle)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Left)
                    // Its text lines up with the card's edge.
                    .margin(Thickness::new(-12.0, -4.0, 0.0, -4.0))
                    .automation_name(name)
                    .on_click(context.message(Message::OpenDevice(device.id)))
                    .content(
                        StackPanel::new()
                            .spacing(2.0)
                            .children((body(name), secondary(caption).font_size(12.0))),
                    ),
                Button::new()
                    .grid_column(1)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Center)
                    .automation_name(remove_label)
                    .on_click(context.message(Message::AskRemove(device.id)))
                    .content(SymbolIcon::new().symbol(Symbol::Delete))
                    .tooltip(t("Remove", "Retirer")),
            )),
    )
}

/// Asks before removing a device, naming it. The dialog's own buttons
/// cannot be coloured, so both buttons are in its content, Remove in red.
/// Only in the tree while open:
/// a closed dialog would still take room on the page.
fn confirm_removal(weft: &Weft, context: &ViewContext<Weft>) -> Option<View> {
    let device = weft
        .removing
        .and_then(|id| weft.snapshot.devices.iter().find(|device| device.id == id))?;
    let name = words::device_name(device);
    let (title, text) = if is_french() {
        (
            format!("Retirer {name} ?"),
            format!(
                "{name} ne recevra plus ce que vous copiez ici, et ce qu'il envoie sera ignoré. Vous pourrez l'associer de nouveau."
            ),
        )
    } else {
        (
            format!("Remove {name}?"),
            format!(
                "{name} will stop receiving what you copy here, and what it sends will be ignored. You can pair it again later."
            ),
        )
    };
    let buttons = StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .horizontal_alignment(HorizontalAlignment::Right)
        .children((
            button(
                t("Cancel", "Annuler"),
                context.message(Message::CancelRemove),
            ),
            danger_button(
                t("Remove", "Retirer"),
                context.message(Message::ConfirmRemove),
            ),
        ));
    let dialog = ContentDialog::new()
        .is_open(weft.removal_hiding.is_none())
        .title(title)
        // Escape closes it too.
        .on_closed(context.callback(|_| Message::RemovalHidden))
        .content(column(24.0, vec![body(text).into(), buttons]));
    Some(dialog)
}

pub fn view(weft: &Weft, context: &mut ViewContext<Weft>) -> View {
    let pair = context.message(Message::StartPairing);
    let mut views: Vec<View> = added(weft, context).into_iter().collect();
    views.push(status(weft));
    let devices = &weft.snapshot.devices;
    if devices.is_empty() {
        views.push(column(
            16.0,
            vec![
                body(t(
                    "Weft carries the text you copy between your phone and your computers, through an encrypted relay.",
                    "Weft transmet le texte que vous copiez entre votre téléphone et vos ordinateurs, par un relais chiffré.",
                ))
                .into(),
                body(t(
                    "Pair a device to start: scan a code with your phone, or enter one from another computer.",
                    "Associez un appareil pour commencer : scannez un code avec votre téléphone, ou saisissez celui d'un autre ordinateur.",
                ))
                .into(),
                primary_button(t("Pair a device", "Associer un appareil"), pair),
            ],
        ));
    } else {
        let rows = devices
            .iter()
            .map(|device| KeyedView::new(hex(&device.id), device_row(weft, device, context)));
        let list = column(
            8.0,
            vec![
                label(t("Paired devices", "Appareils associés")).into(),
                StackPanel::new().spacing(4.0).keyed_children(rows),
            ],
        );
        views.push(column(
            16.0,
            vec![
                list,
                button(t("Pair a device", "Associer un appareil"), pair),
            ],
        ));
    }
    views.extend(confirm_removal(weft, context));
    column(24.0, views)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_banner_distinguishes_joining_and_inviting() {
        assert_eq!(
            added_text(PairingKind::Joined, Some("Phone")),
            if is_french() {
                "Associé à Phone"
            } else {
                "Paired with Phone"
            }
        );
        assert_eq!(
            added_text(PairingKind::Joined, None),
            t("Paired with a new device", "Associé à un nouvel appareil")
        );
        for kind in [PairingKind::Invited, PairingKind::JoinedOther] {
            assert_eq!(
                added_text(kind, Some("Phone")),
                if is_french() {
                    "Ajouté au groupe de Phone"
                } else {
                    "Joined Phone's group"
                }
            );
            assert_eq!(
                added_text(kind, None),
                t("Joined the group", "Ajouté au groupe")
            );
        }
    }
}
