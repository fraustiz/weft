//! The pairing view, in place of the device list: the code of this device
//! to scan or copy, and a field for the code of another one.

use std::time::Instant;

use windows_reactor::*;

use super::app::{Message, Weft};
use super::widgets::{body, button, column, label, primary_button, row, secondary, title, waiting};
use super::words;
use crate::i18n::t;
use crate::worker::PairingState;

/// This device's code: the QR card, when it stops working, the code as
/// text and a way to copy it.
fn showing(weft: &Weft, code: &str, expires_at: Instant, context: &ViewContext<Weft>) -> View {
    let mut views = vec![
        body(t(
            "Scan this code with Weft on your phone. On a computer, enter the code instead.",
            "Scannez ce code avec Weft sur votre téléphone. Sur un ordinateur, saisissez plutôt le code.",
        ))
        .into(),
    ];
    if let Some((_, card)) = &weft.qr {
        views.push(card.clone());
    }
    let copy = if weft.copied {
        t("Copied", "Copié")
    } else {
        t("Copy code", "Copier le code")
    };
    views.push(column(
        8.0,
        vec![
            secondary(words::expiry(expires_at, Instant::now()).0).into(),
            secondary(code)
                .font_size(12.0)
                .max_lines(2)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .is_text_selection_enabled(true)
                .automation_name(t("Pairing code", "Code d'association"))
                .into(),
        ],
    ));
    views.push(row(vec![
        button(copy, context.message(Message::CopyCode)),
        button(
            t("Cancel", "Annuler"),
            context.message(Message::ClosePairing),
        ),
    ]));
    views.push(join_field(weft, context));
    column(16.0, views)
}

/// The second way in: the code another device shows.
fn join_field(weft: &Weft, context: &ViewContext<Weft>) -> View {
    let entered = !weft.join_code.trim().is_empty();
    column(
        8.0,
        vec![
            label(t(
                "Enter a code from another device",
                "Saisir le code d'un autre appareil",
            ))
            .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
            .into(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(8.0)
                .children((
                    TextBox::new()
                        .grid_column(0)
                        .text(weft.join_code.clone())
                        .placeholder_text("WEFT1:…")
                        .automation_name(t("Code from another device", "Code d'un autre appareil"))
                        .on_text_changed(context.callback(Message::JoinCodeEdited)),
                    Button::new()
                        .grid_column(1)
                        .is_enabled(entered)
                        .on_click(context.message(Message::Join))
                        .content(t("Join", "Rejoindre")),
                )),
        ],
    )
}

fn failed(text: String, context: &ViewContext<Weft>) -> View {
    column(
        16.0,
        vec![
            InfoBar::new()
                .severity(InfoBarSeverity::Error)
                .is_open(true)
                .is_closable(false)
                .title(t("Pairing failed", "L'association a échoué"))
                .message(text)
                .into(),
            row(vec![
                primary_button(
                    t("Try again", "Réessayer"),
                    context.message(Message::StartPairing),
                ),
                button(
                    t("Cancel", "Annuler"),
                    context.message(Message::ClosePairing),
                ),
            ]),
        ],
    )
}

pub fn view(weft: &Weft, context: &mut ViewContext<Weft>) -> View {
    let content = match &weft.snapshot.pairing {
        PairingState::ShowingCode { code, expires_at } => showing(weft, code, *expires_at, context),
        PairingState::Joining => waiting(t(
            "Joining: waiting for the other device to answer",
            "Connexion : en attente de la réponse de l'autre appareil",
        )),
        PairingState::Failed(failure) => failed(words::failure(failure), context),
        // Asked for, not there yet; or added, and the list comes back.
        PairingState::Idle | PairingState::Success { .. } => {
            waiting(t("Getting a code", "Création d'un code"))
        }
    };
    column(
        16.0,
        vec![
            title(t("Pair a device", "Associer un appareil")).into(),
            content,
        ],
    )
}
