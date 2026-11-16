//! One device, in place of the list: its battery and network, what this
//! computer can do with it, and the files on their way. Files dropped on
//! the view are sent to it. Also the notice shown while a device rings
//! this computer.

use weft_cli::hex;
use windows_reactor::*;

use super::app::{Message, Weft};
use super::widgets::{
    body, button, card, column, label, primary_button, row, secondary, status_mark, title,
};
use super::words;
use crate::i18n::{is_french, t};
use crate::packets::{Battery, Connectivity};
use crate::worker::{PeerInfo, TransferView};

/// The bars icon for a signal of 0 to 4.
fn bars(signal: u8) -> Symbol {
    match signal {
        0 => Symbol::ZeroBars,
        1 => Symbol::OneBar,
        2 => Symbol::TwoBars,
        3 => Symbol::ThreeBars,
        _ => Symbol::FourBars,
    }
}

/// The battery as a figure, the one large number of the view.
fn battery_view(battery: &Battery) -> Option<View> {
    let (figure, state) = words::battery(battery)?;
    let mut views = vec![
        secondary(t("Battery", "Batterie")).font_size(12.0).into(),
        body(figure).font_size(28.0).into(),
    ];
    if let Some(state) = state {
        views.push(secondary(state).font_size(12.0).into());
    }
    Some(column(0.0, views))
}

fn network_view(connectivity: &Connectivity) -> Option<View> {
    let network = words::network(connectivity)?;
    let mut line = vec![
        body(network)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    ];
    if let Some(signal) = connectivity.signal {
        let label = if is_french() {
            format!("Signal : {signal} sur 4")
        } else {
            format!("Signal: {signal} of 4")
        };
        line.insert(
            0,
            SymbolIcon::new()
                .symbol(bars(signal))
                .opacity(0.7)
                .automation_name(label)
                .into(),
        );
    }
    Some(column(
        4.0,
        vec![
            secondary(t("Network", "Réseau")).font_size(12.0).into(),
            row(line),
        ],
    ))
}

/// Battery and network, when the device sends them and this computer
/// shows them.
fn status(weft: &Weft, peer: &PeerInfo) -> Option<View> {
    let sends = |kind| {
        peer.capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.sends(kind))
    };
    if !weft.snapshot.features.battery || !(sends("battery") || sends("connectivity")) {
        return None;
    }
    let parts: Vec<View> = peer
        .battery
        .as_ref()
        .and_then(battery_view)
        .into_iter()
        .chain(peer.connectivity.as_ref().and_then(network_view))
        .collect();
    if parts.is_empty() {
        return Some(card(secondary(t(
            "No battery or network news yet",
            "Pas encore de nouvelles de la batterie ni du réseau",
        ))));
    }
    let columns = vec![GridLength::STAR; parts.len()];
    let cells = parts.into_iter().enumerate().map(|(index, part)| {
        KeyedView::new(index, Border::new().grid_column(index as i32).content(part))
    });
    Some(card(
        Grid::new()
            .columns(columns)
            .column_spacing(16.0)
            .keyed_children(cells),
    ))
}

/// Ring and Send files, each only if both devices handle it.
fn actions(
    weft: &Weft,
    device: [u8; 16],
    name: &str,
    peer: &PeerInfo,
    context: &ViewContext<Weft>,
) -> View {
    if peer.capabilities.is_none() {
        let text = if is_french() {
            format!("Les actions s'affichent quand {name} est en ligne")
        } else {
            format!("Actions show once {name} is online")
        };
        return secondary(text).into();
    }
    let features = weft.snapshot.features;
    let mut buttons = Vec::new();
    let files = features.files && peer.handles("file.offer");
    if files {
        buttons.push(primary_button(
            t("Send files…", "Envoyer des fichiers…"),
            context.message(Message::PickFiles),
        ));
    }
    if features.ring && peer.handles("ring") {
        let ringing = weft.rang.is_some_and(|(rang, _)| rang == device);
        buttons.push(if ringing {
            button(
                t("Stop ringing", "Arrêter la sonnerie"),
                context.message(Message::StopRinging),
            )
        } else {
            button(t("Ring", "Faire sonner"), context.message(Message::Ring))
        });
    }
    if buttons.is_empty() {
        let text = if is_french() {
            format!("Rien à faire avec {name} pour l'instant")
        } else {
            format!("Nothing to do with {name} for now")
        };
        return secondary(text).into();
    }
    let mut views = vec![row(buttons)];
    if files {
        views.push(
            secondary(t(
                "Or drop files on this window",
                "Ou déposez des fichiers sur cette fenêtre",
            ))
            .font_size(12.0)
            .into(),
        );
    }
    column(8.0, views)
}

fn transfer_row(transfer: &TransferView, context: &ViewContext<Weft>) -> View {
    let active = transfer.stage.is_active();
    let (icon, direction) = if transfer.outgoing {
        (Symbol::Upload, t("Sending", "Envoi"))
    } else {
        (Symbol::Download, t("Receiving", "Réception"))
    };
    let mut lines = vec![
        body(transfer.name.clone())
            .max_lines(1)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .into(),
    ];
    if active {
        let bar = if transfer.size == 0 || transfer.done == 0 {
            ProgressBar::new().is_indeterminate(true)
        } else {
            ProgressBar::new()
                .maximum(transfer.size as f64)
                .value(transfer.done as f64)
        };
        lines.push(bar.into());
    }
    lines.push(
        secondary(words::transfer_status(transfer))
            .font_size(12.0)
            .into(),
    );
    let mut cells: Vec<View> = vec![
        SymbolIcon::new()
            .symbol(icon)
            .opacity(0.7)
            .automation_name(direction)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(0)
            .into(),
        Border::new().grid_column(1).content(column(4.0, lines)),
    ];
    if active {
        let cancel = if is_french() {
            format!("Annuler {}", transfer.name)
        } else {
            format!("Cancel {}", transfer.name)
        };
        cells.push(
            Button::new()
                .grid_column(2)
                .style(ButtonStyle::Subtle)
                .vertical_alignment(VerticalAlignment::Center)
                .automation_name(cancel)
                .on_click(context.message(Message::CancelTransfer(transfer.id)))
                .content(SymbolIcon::new().symbol(Symbol::Cancel))
                .tooltip(t("Cancel", "Annuler")),
        );
    }
    card(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .keyed_children(super::widgets::keyed(cells)),
    )
}

pub fn view(weft: &Weft, device: [u8; 16], context: &mut ViewContext<Weft>) -> View {
    let snapshot = &weft.snapshot;
    let Some(known) = snapshot.devices.iter().find(|known| known.id == device) else {
        return column(0.0, Vec::new());
    };
    let name = words::device_name(known).to_owned();
    let peer = snapshot.peer(&device);
    let back = if is_french() {
        "Retour aux appareils"
    } else {
        "Back to devices"
    };
    let header = Grid::new()
        .columns([GridLength::Auto, GridLength::STAR])
        .column_spacing(8.0)
        .children((
            Button::new()
                .grid_column(0)
                .style(ButtonStyle::Subtle)
                .vertical_alignment(VerticalAlignment::Center)
                // Its arrow lines up with the cards below.
                .margin(Thickness::new(-12.0, 0.0, 0.0, 0.0))
                .automation_name(back)
                .on_click(context.message(Message::CloseDevice))
                .content(SymbolIcon::new().symbol(Symbol::Back))
                .tooltip(back),
            StackPanel::new()
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    title(name.clone()),
                    secondary(words::device_id(known)).font_size(12.0),
                )),
        ));
    let mut views: Vec<View> = vec![header];
    views.extend(status(weft, &peer));
    views.push(actions(weft, device, &name, &peer, context));
    let transfers: Vec<KeyedView> = snapshot
        .transfers
        .iter()
        .filter(|transfer| transfer.device == device)
        .map(|transfer| KeyedView::new(hex(&transfer.id), transfer_row(transfer, context)))
        .collect();
    if !transfers.is_empty() {
        views.push(column(
            8.0,
            vec![
                label(t("Files", "Fichiers")).into(),
                StackPanel::new().spacing(4.0).keyed_children(transfers),
            ],
        ));
    }
    let page = column(24.0, views);
    if !(snapshot.features.files && peer.handles("file.offer")) {
        return page;
    }
    let caption = if is_french() {
        format!("Envoyer à {name}")
    } else {
        format!("Send to {name}")
    };
    // A background, even a clear one, is what makes it a drop target.
    Border::new()
        .background(Color::argb(0, 0, 0, 0))
        .drop_policy(
            DragDropPolicy::new()
                .storage_items(DragDropAction::new(DragDropOperation::Copy).caption(caption)),
        )
        .on_drop(context.callback(Message::Dropped))
        .content(page)
}

/// While a device rings this computer: who, and the button that stops it,
/// above every page.
pub fn ringing(weft: &Weft, context: &ViewContext<Weft>) -> Option<View> {
    let device = weft.snapshot.ringing?;
    let name = weft
        .snapshot
        .devices
        .iter()
        .find(|known| known.id == device)
        .map(|known| words::device_name(known).to_owned())
        .unwrap_or_else(|| t("A device", "Un appareil").to_owned());
    let text = if is_french() {
        format!("{name} fait sonner cet ordinateur")
    } else {
        format!("{name} is ringing this computer")
    };
    Some(card(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                Border::new()
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(status_mark(true)),
                StackPanel::new()
                    .grid_column(1)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        label(text),
                        secondary(t(
                            "It stops by itself after a minute",
                            "Elle s'arrête d'elle-même au bout d'une minute",
                        ))
                        .font_size(12.0),
                    )),
                Border::new()
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(primary_button(
                        t("Found it", "Trouvé"),
                        context.message(Message::FoundIt),
                    )),
            )),
    ))
}
