//! One device, in place of the list: its battery and network, what this
//! computer can do with it, what it plays and its volume, and the files on
//! their way. Files dropped on the view are sent to it. Also the notice
//! shown while a device rings this computer.

use weft_cli::hex;
use windows_reactor::*;

use super::app::{Draft, Message, Weft};
use super::widgets::{
    body, button, card, column, label, primary_button, row, secondary, status_mark, title,
};
use super::words;
use crate::i18n::{is_french, t};
use crate::media;
use crate::packets::{Battery, Connectivity, MAX_URL_LEN, MediaAction, Player, is_shareable_url};
use crate::worker::{PeerInfo, TransferView};

/// The side of a player's cover, in effective pixels.
const COVER_SIZE: f64 = 64.0;

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

/// The box for a link to send: Send only once it is a link the other
/// device will open.
fn link_box(link: &str, context: &ViewContext<Weft>) -> View {
    let url = link.trim();
    let valid = is_shareable_url(url);
    let hint = if url.is_empty() || valid {
        t(
            "A web link, opened on the other device",
            "Un lien web, ouvert sur l'autre appareil",
        )
        .to_owned()
    } else if is_french() {
        format!("Liens http et https uniquement, {MAX_URL_LEN} octets au plus")
    } else {
        format!("Only http and https links, up to {MAX_URL_LEN} bytes")
    };
    card(column(
        8.0,
        vec![
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    TextBox::new()
                        .grid_column(0)
                        .text(link.to_owned())
                        .placeholder_text("https://…")
                        .automation_name(t("Link to send", "Lien à envoyer"))
                        .on_text_changed(context.callback(Message::LinkEdited)),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(valid)
                        .on_click(context.message(Message::SendLink))
                        .content(t("Send", "Envoyer")),
                    Button::new()
                        .grid_column(2)
                        .on_click(context.message(Message::CloseLink))
                        .content(t("Cancel", "Annuler")),
                )),
            secondary(hint).font_size(12.0).into(),
        ],
    ))
}

/// Send files, Send link and Messages, each only if both devices handle
/// it. Ring and Ping act on the device itself, so they sit in its header.
fn actions(weft: &Weft, name: &str, peer: &PeerInfo, context: &ViewContext<Weft>) -> View {
    if peer.capabilities.is_none() {
        let text = if is_french() {
            format!("Les actions s'affichent quand {name} est en ligne")
        } else {
            format!("Actions show once {name} is online")
        };
        return secondary(text).into();
    }
    let features = weft.snapshot.features;
    let mut file_buttons = Vec::new();
    let files = features.files && peer.handles("file.offer");
    if files {
        file_buttons.push(primary_button(
            t("Send files…", "Envoyer des fichiers…"),
            context.message(Message::PickFiles),
        ));
        file_buttons.push(button(
            t("Paste files", "Coller des fichiers"),
            context.message(Message::PasteFiles),
        ));
    }
    let mut other_buttons = Vec::new();
    let links = features.links && peer.handles("share.url");
    if links {
        other_buttons.push(button(
            t("Send link…", "Envoyer un lien…"),
            context.message(Message::OpenLink),
        ));
    }
    if features.sms && peer.handles("sms.conversations.request") {
        other_buttons.push(button(
            t("Messages", "Messages"),
            context.message(Message::OpenMessages),
        ));
    }
    if file_buttons.is_empty() && other_buttons.is_empty() {
        let text = if is_french() {
            format!("Rien à faire avec {name} pour l'instant")
        } else {
            format!("Nothing to do with {name} for now")
        };
        return secondary(text).into();
    }
    let mut views = Vec::new();
    if !file_buttons.is_empty() {
        views.push(row(file_buttons));
    }
    if !other_buttons.is_empty() {
        views.push(row(other_buttons));
    }
    if links && let Some(link) = &weft.link {
        views.push(link_box(link, context));
    }
    if files {
        let empty_paste = weft
            .paste_empty_until
            .is_some_and(|until| std::time::Instant::now() < until);
        let hint = if empty_paste {
            t(
                "No files on the clipboard",
                "Aucun fichier dans le presse-papiers",
            )
        } else {
            t(
                "Tip: drop files onto this window to send them",
                "Astuce : déposez des fichiers sur la fenêtre pour les envoyer",
            )
        };
        views.push(secondary(hint).font_size(12.0).into());
    }
    column(8.0, views)
}

/// Where the player shown on `device` is: the position just asked for,
/// else the one it reported, moved on with time.
pub fn position_shown(weft: &Weft, device: [u8; 16]) -> Option<u64> {
    let media = weft.snapshot.peer(&device).media?;
    let player = media.current()?;
    if let Some((position, at)) = Draft::for_device(&weft.seek_draft, device) {
        let asked = Player {
            position: Some(position),
            ..player.clone()
        };
        return media::position(&asked, at.elapsed());
    }
    media::position(player, media.at.elapsed())
}

/// The volume shown for `device`: the one just asked for, else the one it
/// reported.
pub fn volume_shown(weft: &Weft, device: [u8; 16]) -> Option<u8> {
    Draft::for_device(&weft.volume_draft, device)
        .map(|(level, _)| level)
        .or_else(|| weft.snapshot.peer(&device).volume?.level)
}

/// A quiet icon button for the device header, a Segoe Fluent glyph.
fn header_button(glyph: &str, name: &str, on_click: Callback<()>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .automation_name(name)
        .on_click(on_click)
        .content(FontIcon::new().glyph(glyph))
        .tooltip(name)
}

/// Ring (or Stop ringing) and Ping, each only if both devices handle it,
/// for the right of the device header. Nothing until it is online.
fn header_actions(
    weft: &Weft,
    device: [u8; 16],
    peer: &PeerInfo,
    context: &ViewContext<Weft>,
) -> Vec<View> {
    let mut buttons = Vec::new();
    if peer.capabilities.is_none() {
        return buttons;
    }
    if weft.snapshot.features.ring && peer.handles("ring") {
        let ringing = weft.rang.is_some_and(|(rang, _)| rang == device);
        buttons.push(if ringing {
            // RingerSilent
            header_button(
                "\u{E7ED}",
                t("Stop ringing", "Arrêter la sonnerie"),
                context.message(Message::StopRinging),
            )
        } else {
            // Ringer
            header_button(
                "\u{EA8F}",
                t("Ring", "Faire sonner"),
                context.message(Message::Ring),
            )
        });
    }
    if peer.handles("ping") {
        // Streaming, waves from a point
        buttons.push(header_button(
            "\u{E93E}",
            t("Ping", "Ping"),
            context.message(Message::Ping),
        ));
    }
    buttons
}

/// A player button: an icon, with its name for screen readers and as a
/// tooltip.
fn player_button(symbol: Symbol, name: &str, enabled: bool, on_click: Callback<()>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .is_enabled(enabled)
        .automation_name(name)
        .on_click(on_click)
        .content(SymbolIcon::new().symbol(symbol))
        .tooltip(name)
}

/// What the device plays: the first player playing, with its controls.
/// `None` until the device has said, or when it does not share players.
fn now_playing(
    weft: &Weft,
    device: [u8; 16],
    name: &str,
    peer: &PeerInfo,
    context: &ViewContext<Weft>,
) -> Option<View> {
    if !weft.snapshot.features.media || !peer.sends("media") {
        return None;
    }
    let media = peer.media.as_ref()?;
    let Some(player) = media.current() else {
        let text = if is_french() {
            format!("Rien en lecture sur {name}")
        } else {
            format!("Nothing playing on {name}")
        };
        return Some(card(secondary(text)));
    };
    let one_line = |text: TextBlock| -> View {
        text.max_lines(1)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .into()
    };
    let title = if player.title.trim().is_empty() {
        t("Untitled", "Sans titre").to_owned()
    } else {
        player.title.clone()
    };
    let mut lines = Vec::new();
    if !player.app.trim().is_empty() {
        lines.push(one_line(secondary(player.app.clone()).font_size(12.0)));
    }
    lines.push(one_line(body(title)));
    if !player.artist.trim().is_empty() {
        lines.push(one_line(secondary(player.artist.clone())));
    }
    let text = column(2.0, lines);
    // The cover, once the device sent the image its `art` names.
    let heading = match peer.cover(player) {
        Some(cover) => Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(12.0)
            .children((
                Image::new()
                    .grid_column(0)
                    .width(COVER_SIZE)
                    .height(COVER_SIZE)
                    .stretch(Stretch::UniformToFill)
                    .vertical_alignment(VerticalAlignment::Center)
                    .source_data(EncodedImage::new(cover.data.clone())),
                Border::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(text),
            )),
        None => text,
    };
    let mut views = vec![heading];
    if player.can_seek
        && let (Some(length), Some(position)) = (player.length, position_shown(weft, device))
    {
        let time = |ms| {
            secondary(words::track_time(ms))
                .font_size(12.0)
                .vertical_alignment(VerticalAlignment::Center)
        };
        views.push(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(12.0)
                .children((
                    time(position).grid_column(0),
                    Slider::new()
                        .grid_column(1)
                        .minimum(0.0)
                        .maximum((length / 1000) as f64)
                        .value((position / 1000) as f64)
                        .vertical_alignment(VerticalAlignment::Center)
                        .automation_name(t("Position", "Position"))
                        .on_value_changed(context.callback(Message::Seek)),
                    time(length).grid_column(2),
                )),
        );
    }
    let (toggle, toggle_symbol, toggle_name, toggle_enabled) = if player.playing {
        (
            MediaAction::Pause,
            Symbol::Pause,
            t("Pause", "Pause"),
            player.can_pause,
        )
    } else {
        (
            MediaAction::Play,
            Symbol::Play,
            t("Play", "Lecture"),
            player.can_play,
        )
    };
    views.push(
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .keyed_children(super::widgets::keyed(vec![
                player_button(
                    Symbol::Previous,
                    t("Previous", "Précédent"),
                    player.can_previous,
                    context.message(Message::Media(MediaAction::Previous)),
                ),
                player_button(
                    toggle_symbol,
                    toggle_name,
                    toggle_enabled,
                    context.message(Message::Media(toggle)),
                ),
                player_button(
                    Symbol::Next,
                    t("Next", "Suivant"),
                    player.can_next,
                    context.message(Message::Media(MediaAction::Next)),
                ),
            ])),
    );
    Some(card(column(8.0, views)))
}

/// The device's volume, with a slider and a mute button. `None` until the
/// device has said, or when it cannot be changed from here.
fn volume(
    weft: &Weft,
    device: [u8; 16],
    peer: &PeerInfo,
    context: &ViewContext<Weft>,
) -> Option<View> {
    if !weft.snapshot.features.volume || !peer.handles("volume.set") {
        return None;
    }
    let muted = peer.volume?.muted.unwrap_or(false);
    let level = volume_shown(weft, device)?;
    let (symbol, mute_name) = if muted {
        (Symbol::Mute, t("Unmute", "Réactiver le son"))
    } else {
        (Symbol::Volume, t("Mute", "Couper le son"))
    };
    Some(card(
        Grid::new()
            .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((
                Button::new()
                    .grid_column(0)
                    .style(ButtonStyle::Subtle)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(-8.0, 0.0, 0.0, 0.0))
                    .automation_name(mute_name)
                    .on_click(context.message(Message::PhoneMute(!muted)))
                    .content(SymbolIcon::new().symbol(symbol))
                    .tooltip(mute_name),
                Slider::new()
                    .grid_column(1)
                    .minimum(0.0)
                    .maximum(100.0)
                    .value(f64::from(level))
                    .vertical_alignment(VerticalAlignment::Center)
                    .automation_name(t("Volume", "Volume"))
                    .on_value_changed(context.callback(Message::PhoneVolume)),
                // A fixed width, so the slider keeps its length from 9 to 10.
                Border::new()
                    .grid_column(2)
                    .width(28.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(
                        secondary(level.to_string())
                            .horizontal_alignment(HorizontalAlignment::Right),
                    ),
            )),
    ))
}

/// The device's hold on this computer, and typing on it from here: the
/// permission whenever it can send input, the box when it can be typed on.
fn control(
    weft: &Weft,
    device: [u8; 16],
    name: &str,
    peer: &PeerInfo,
    context: &ViewContext<Weft>,
) -> Option<View> {
    let mut views = Vec::new();
    let remote = ["input.pointer", "input.click", "input.key", "presenter"];
    if remote.iter().any(|kind| peer.sends(kind)) {
        let title = if is_french() {
            format!("Laisser {name} contrôler cet ordinateur")
        } else {
            format!("Let {name} control this computer")
        };
        views.push(super::settings::switch(
            &title,
            t(
                "Pointer, keyboard and slideshows. Windows run as administrator ignore it",
                "Pointeur, clavier et diaporamas. Les fenêtres lancées en administrateur l'ignorent",
            ),
            weft.snapshot.control.contains(&device),
            context.callback(Message::SetControl),
        ));
    }
    if peer.handles("input.key") {
        let placeholder = if is_french() {
            format!("Ce que vous tapez s'écrit sur {name}")
        } else {
            format!("What you type here is typed on {name}")
        };
        views.push(card(column(
            8.0,
            vec![
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .column_spacing(8.0)
                    .children((
                        TextBox::new()
                            .grid_column(0)
                            .text(weft.typed.clone())
                            .placeholder_text(placeholder)
                            .accepts_return(true)
                            .text_wrapping(TextWrapping::Wrap)
                            .automation_name(t("Type on phone", "Taper sur le téléphone"))
                            .on_text_changed(context.callback(Message::TypedEdited)),
                        Border::new()
                            .grid_column(1)
                            .vertical_alignment(VerticalAlignment::Bottom)
                            .content(button(
                                t("Clear", "Effacer"),
                                context.message(Message::ClearTyped),
                            )),
                    )),
                secondary(t(
                    "Needs the Weft keyboard chosen on the phone. Clear empties this box only",
                    "Le clavier Weft doit être choisi sur le téléphone. Effacer ne vide que ce champ",
                ))
                .font_size(12.0)
                .into(),
            ],
        )));
    }
    if views.is_empty() {
        return None;
    }
    Some(column(
        8.0,
        vec![
            label(t("Remote control", "Contrôle à distance")).into(),
            column(4.0, views),
        ],
    ))
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
        .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
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
            StackPanel::new()
                .grid_column(2)
                .orientation(Orientation::Horizontal)
                .spacing(4.0)
                .vertical_alignment(VerticalAlignment::Center)
                .keyed_children(super::widgets::keyed(header_actions(
                    weft, device, &peer, context,
                ))),
        ));
    let mut views: Vec<View> = vec![header];
    views.extend(status(weft, &peer));
    views.push(actions(weft, &name, &peer, context));
    let media: Vec<View> = now_playing(weft, device, &name, &peer, context)
        .into_iter()
        .chain(volume(weft, device, &peer, context))
        .collect();
    if !media.is_empty() {
        views.push(column(
            8.0,
            vec![label(t("Media", "Média")).into(), column(4.0, media)],
        ));
    }
    views.extend(control(weft, device, &name, &peer, context));
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
