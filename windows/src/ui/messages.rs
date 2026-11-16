//! A phone's SMS, in place of its details: the conversations, one thread
//! with its messages and a reply box, or a new message whose recipient is
//! picked from the phone's contacts.
//!
//! Messages are bubbles: the phone's correspondents on the card surface,
//! this side's in cream with ink text, so the two read apart in both
//! themes without a second accent. Unread conversations carry the brand
//! pixel.

use windows_reactor::*;

use super::app::{Message, MessagesView, OpenThread, Weft};
use super::widgets::{CREAM, INK};
use super::widgets::{body, card, column, primary_button, secondary, status_mark, title, waiting};
use super::words;
use crate::i18n::{is_french, t};
use crate::packets::Conversation;
use crate::sms::{self, SendState, Sms};

/// How many contacts the recipient box offers at once.
const SUGGESTIONS: usize = 8;
/// The widest a bubble grows.
const BUBBLE_WIDTH: f64 = 340.0;
/// Messages closer than this, from the same side, share one time line.
const RUN: i64 = 10 * 60 * 1000;

/// A back arrow, a title and a line under it, as the device view has.
fn header(heading: String, subtitle: String, back: Message, context: &ViewContext<Weft>) -> View {
    let name = t("Back", "Retour");
    Grid::new()
        .columns([GridLength::Auto, GridLength::STAR])
        .column_spacing(8.0)
        .children((
            Button::new()
                .grid_column(0)
                .style(ButtonStyle::Subtle)
                .vertical_alignment(VerticalAlignment::Center)
                // Its arrow lines up with the cards below.
                .margin(Thickness::new(-12.0, 0.0, 0.0, 0.0))
                .automation_name(name)
                .on_click(context.message(back))
                .content(SymbolIcon::new().symbol(Symbol::Back))
                .tooltip(name),
            StackPanel::new()
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Center)
                .keyed_children(super::widgets::keyed(
                    std::iter::once(one_line(title(heading)).into())
                        .chain(
                            (!subtitle.is_empty())
                                .then(|| secondary(subtitle).font_size(12.0).into()),
                        )
                        .collect(),
                )),
        ))
}

fn one_line(text: TextBlock) -> TextBlock {
    text.max_lines(1)
        .text_trimming(TextTrimming::CharacterEllipsis)
}

/// One conversation: the unread pixel, who, the last message, when.
fn conversation_row(conversation: &Conversation, context: &ViewContext<Weft>) -> View {
    let who = conversation.title();
    let mark = if conversation.unread {
        status_mark(true)
    } else {
        Border::new().width(10.0).into()
    };
    let mut name = one_line(body(who.clone()));
    if conversation.unread {
        name = name.font_weight(FontWeight::SEMI_BOLD);
    }
    let snippet = conversation.snippet.clone().unwrap_or_default();
    Button::new()
        .style(ButtonStyle::Subtle)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .horizontal_content_alignment(HorizontalAlignment::Stretch)
        .automation_name(who)
        .on_click(context.message(Message::OpenThread(conversation.thread)))
        .content(
            Grid::new()
                .columns([GridLength::Auto, GridLength::STAR, GridLength::Auto])
                .column_spacing(12.0)
                .margin(Thickness::new(0.0, 4.0, 0.0, 4.0))
                .children((
                    Border::new()
                        .grid_column(0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .content(mark),
                    StackPanel::new()
                        .grid_column(1)
                        .spacing(2.0)
                        .children((name, one_line(secondary(snippet)))),
                    secondary(words::sms_time(conversation.time, false))
                        .grid_column(2)
                        .font_size(12.0)
                        .vertical_alignment(VerticalAlignment::Top),
                )),
        )
}

/// The conversations, newest first.
fn list(name: &str, sms: &Sms, context: &ViewContext<Weft>) -> View {
    let mut views = vec![
        header(
            t("Messages", "Messages").to_owned(),
            name.to_owned(),
            Message::CloseMessages,
            context,
        ),
        primary_button(
            t("New message", "Nouveau message"),
            context.message(Message::StartConversation),
        ),
    ];
    match &sms.conversations {
        None => views.push(waiting(if is_french() {
            "En attente des conversations du téléphone"
        } else {
            "Waiting for the phone's conversations"
        })),
        Some(conversations) if conversations.is_empty() => {
            views.push(secondary(t("No conversations", "Aucune conversation")).into())
        }
        Some(conversations) => {
            let rows: Vec<KeyedView> = conversations
                .iter()
                .map(|conversation| {
                    KeyedView::new(
                        conversation.thread.to_string(),
                        conversation_row(conversation, context),
                    )
                })
                .collect();
            views.push(
                Border::new()
                    .background(ThemeBrush::CardBackground)
                    .border_brush(ThemeBrush::CardStroke)
                    .border_thickness(Thickness::uniform(1.0))
                    .corner_radius(8.0)
                    .padding(Thickness::uniform(4.0))
                    .content(StackPanel::new().spacing(2.0).keyed_children(rows)),
            );
        }
    }
    column(16.0, views)
}

/// One message: a bubble on its side, and the line under it when it has
/// one.
fn bubble(text: &str, outgoing: bool, meta: Option<(String, bool)>) -> View {
    let side = if outgoing {
        HorizontalAlignment::Right
    } else {
        HorizontalAlignment::Left
    };
    let mut text = body(text.to_owned()).is_text_selection_enabled(true);
    let mut shape = Border::new()
        .corner_radius(12.0)
        .padding(Thickness::new(12.0, 8.0, 12.0, 8.0))
        .max_width(BUBBLE_WIDTH)
        .horizontal_alignment(side);
    if outgoing {
        text = text.foreground(INK);
        shape = shape.background(CREAM);
    } else {
        shape = shape
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0));
    }
    let mut views: Vec<View> = vec![shape.content(text)];
    if let Some((line, failed)) = meta {
        let line = if failed {
            body(line).foreground(ThemeBrush::SystemCritical)
        } else {
            secondary(line)
        };
        views.push(line.font_size(12.0).horizontal_alignment(side).into());
    }
    column(4.0, views)
}

/// The people of a thread, as its title says them.
fn people(sms: &Sms, open: &OpenThread) -> String {
    match open.thread.and_then(|thread| sms.conversation(thread)) {
        Some(conversation) => conversation.title(),
        None => open.addresses.join(", "),
    }
}

/// The reply box and its send button.
fn compose(weft: &Weft, view: &MessagesView, context: &ViewContext<Weft>) -> View {
    Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .column_spacing(8.0)
        .children((
            TextBox::new()
                .grid_column(0)
                .element_ref(&weft.compose)
                .text(view.draft.clone())
                .placeholder_text(t("Text message", "SMS"))
                .accepts_return(true)
                .text_wrapping(TextWrapping::Wrap)
                .automation_name(t("Message", "Message"))
                .on_text_changed(context.callback(Message::SmsDraftEdited)),
            Border::new()
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Bottom)
                .content(primary_button(
                    t("Send", "Envoyer"),
                    context.message(Message::SendSms),
                )),
        ))
}

/// A thread: "Load earlier", the messages oldest first, those sent from
/// here with where they stand, and the reply box.
fn thread(
    weft: &Weft,
    view: &MessagesView,
    open: &OpenThread,
    sms: &Sms,
    context: &mut ViewContext<Weft>,
) -> View {
    let mut views = vec![header(
        people(sms, open),
        open.addresses.join(", "),
        Message::CloseThread,
        context,
    )];
    let messages = open
        .thread
        .and_then(|thread| sms.threads.get(&thread))
        .map(Vec::as_slice)
        .unwrap_or_default();
    let complete = open
        .thread
        .is_none_or(|thread| sms.complete.contains(&thread));
    if open.thread.is_some() && !complete && !messages.is_empty() {
        views.push(
            Button::new()
                .style(ButtonStyle::Subtle)
                .horizontal_alignment(HorizontalAlignment::Center)
                .on_click(context.message(Message::LoadEarlier))
                .content(t("Load earlier", "Charger les précédents")),
        );
    }
    if open.thread.is_some() && messages.is_empty() && !complete {
        views.push(waiting(t("Loading", "Chargement")));
    }
    let mut bubbles: Vec<KeyedView> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let next = messages.get(index + 1);
        let last_of_run = next
            .is_none_or(|next| next.outgoing != message.outgoing || next.time - message.time > RUN);
        let failed = message.status.as_deref() == Some("failed");
        let meta = if failed {
            Some((t("Not sent", "Non envoyé").to_owned(), true))
        } else if message.status.as_deref() == Some("pending") {
            Some((t("Sending", "Envoi").to_owned(), false))
        } else {
            last_of_run.then(|| (words::sms_time(message.time, true), false))
        };
        bubbles.push(KeyedView::new(
            format!("m{}", message.id),
            bubble(&message.body, message.outgoing, meta),
        ));
    }
    let outbox = sms
        .outbox
        .iter()
        .filter(|sent| match (sent.thread, open.thread) {
            (Some(own), Some(shown)) => own == shown,
            _ => sms::same_people(&sent.addresses, &open.addresses),
        });
    for sent in outbox {
        let meta = match &sent.state {
            SendState::Pending => (t("Sending", "Envoi").to_owned(), false),
            SendState::Sent => (t("Sent", "Envoyé").to_owned(), false),
            SendState::Failed(reason) => {
                let reason = words::sms_failure(reason.as_deref());
                let line = if is_french() {
                    format!("Non envoyé : {reason}")
                } else {
                    format!("Not sent: {reason}")
                };
                (line, true)
            }
        };
        bubbles.push(KeyedView::new(
            format!("o{}", sent.request_id),
            bubble(&sent.text, true, Some(meta)),
        ));
    }
    if bubbles.is_empty() && complete {
        views.push(secondary(t("No messages yet", "Pas encore de message")).into());
    }
    views.push(StackPanel::new().spacing(8.0).keyed_children(bubbles));
    views.push(compose(weft, view, context));

    // Focusing the reply box scrolls the page to it, under the newest
    // message, when a thread opens or grows.
    let newest = messages.last().map(|message| message.id);
    let compose_box = weft.compose.clone();
    context.use_effect(
        "sms-thread",
        (view.device, open.thread, newest, sms.outbox.len()),
        move || {
            let _ = compose_box.request_focus();
            None
        },
    );
    column(16.0, views)
}

/// A new message: who to, from the contacts or as a number, and the text.
fn new_message(
    weft: &Weft,
    view: &MessagesView,
    name: &str,
    sms: &Sms,
    context: &ViewContext<Weft>,
) -> View {
    let suggestions: Vec<String> = sms::suggestions(&sms.contacts, &view.recipient, SUGGESTIONS)
        .into_iter()
        .map(|suggestion| suggestion.label)
        .collect();
    let typed = !view.recipient.trim().is_empty();
    let caption = if typed && sms::recipient(&sms.contacts, &view.recipient).is_none() {
        t(
            "Pick a contact or type a number",
            "Choisissez un contact ou tapez un numéro",
        )
    } else if sms.contacts.is_empty() {
        t(
            "A phone number. The phone's contacts show here once it sends them",
            "Un numéro de téléphone. Les contacts du téléphone s'affichent ici dès qu'il les envoie",
        )
    } else {
        t(
            "A name from the phone's contacts, or a number",
            "Un nom des contacts du téléphone, ou un numéro",
        )
    };
    let views = vec![
        header(
            t("New message", "Nouveau message").to_owned(),
            name.to_owned(),
            Message::CloseThread,
            context,
        ),
        column(
            4.0,
            vec![
                AutoSuggestBox::new()
                    .text(view.recipient.clone())
                    .placeholder_text(t("To", "À"))
                    .items_source(suggestions)
                    .on_text_changed(context.callback(Message::RecipientEdited))
                    .on_suggestion_chosen(context.callback(Message::RecipientEdited))
                    .into(),
                secondary(caption).font_size(12.0).into(),
            ],
        ),
        compose(weft, view, context),
    ];
    column(16.0, views)
}

pub fn view(weft: &Weft, view: &MessagesView, context: &mut ViewContext<Weft>) -> View {
    let snapshot = &weft.snapshot;
    let name = snapshot
        .devices
        .iter()
        .find(|known| known.id == view.device)
        .map(|known| words::device_name(known).to_owned())
        .unwrap_or_default();
    let peer = snapshot.peer(&view.device);
    if !snapshot.features.sms || !peer.handles("sms.conversations.request") {
        let text = if is_french() {
            format!(
                "Les messages de {name} ne sont pas disponibles. Activez-les dans les réglages de Weft sur le téléphone."
            )
        } else {
            format!(
                "Messages of {name} are not available. Turn them on in Weft's settings on the phone."
            )
        };
        return column(
            16.0,
            vec![
                header(
                    t("Messages", "Messages").to_owned(),
                    name,
                    Message::CloseMessages,
                    context,
                ),
                card(body(text)),
            ],
        );
    }
    let sms = peer.sms.clone();
    match &view.open {
        Some(open) => thread(weft, view, open, &sms, context),
        None if view.composing => new_message(weft, view, &name, &sms, context),
        None => list(&name, &sms, context),
    }
}
