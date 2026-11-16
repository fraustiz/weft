//! The settings page: this device's name, the sync switches, the relay and
//! what the app is.

use windows_reactor::*;

use super::app::{Message, Weft};
use super::widgets::{body, card, column, label, secondary};
use crate::features::Features;
use crate::i18n::t;

/// The repository, shown in About.
const SOURCE: &str = "https://github.com/fraustiz/weft";

/// A switch with its title and what it does, as Windows Settings shows
/// them. The On and Off words are given, or they would follow the system
/// language rather than the app's.
fn switch(title: &str, caption: &str, on: bool, toggled: Callback<bool>) -> View {
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .spacing(2.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((body(title), secondary(caption).font_size(12.0))),
                ToggleSwitch::new()
                    .grid_column(1)
                    .is_on(on)
                    .automation_name(title)
                    .on_toggled(toggled)
                    .vertical_alignment(VerticalAlignment::Center)
                    .slots([
                        SlotView::new(ToggleSwitchSlot::OnContent, t("On", "Activé")),
                        SlotView::new(ToggleSwitchSlot::OffContent, t("Off", "Désactivé")),
                    ]),
            )),
    )
}

fn device_name(weft: &Weft, context: &ViewContext<Weft>) -> View {
    let draft = weft.name_draft.as_deref().unwrap_or(&weft.snapshot.name);
    let changed = draft.trim() != weft.snapshot.name;
    let mut views = vec![
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(8.0)
            .children((
                TextBox::new()
                    .grid_column(0)
                    .text(draft)
                    .automation_name(t("Device name", "Nom de l'appareil"))
                    .on_text_changed(context.callback(Message::NameEdited)),
                Button::new()
                    .grid_column(1)
                    .is_enabled(changed)
                    .on_click(context.message(Message::SaveName))
                    .content(t("Save", "Enregistrer")),
            )),
        secondary(t(
            "How your other devices name this computer",
            "Le nom de cet ordinateur sur vos autres appareils",
        ))
        .font_size(12.0)
        .into(),
    ];
    if let Some(problem) = weft.name_problem {
        views.push(
            body(problem.text())
                .foreground(ThemeBrush::SystemCritical)
                .into(),
        );
    }
    card(column(8.0, views))
}

pub fn view(weft: &Weft, context: &mut ViewContext<Weft>) -> View {
    let snapshot = &weft.snapshot;
    let mut sync = vec![
        switch(
            t("Pause sync", "Mettre en pause"),
            t(
                "Nothing is sent or received until you turn it off",
                "Rien n'est envoyé ni reçu tant que c'est activé",
            ),
            snapshot.pause.all,
            context.callback(Message::SetPaused),
        ),
        switch(
            t("Start with Windows", "Démarrer avec Windows"),
            t(
                "Weft starts in the notification area when you sign in",
                "Weft démarre dans la zone de notification à l'ouverture de session",
            ),
            weft.autostart,
            context.callback(Message::SetAutostart),
        ),
    ];
    if weft.autostart_failed {
        sync.push(
            body(t(
                "Windows refused the change. Try again later.",
                "Windows a refusé la modification. Réessayez plus tard.",
            ))
            .foreground(ThemeBrush::SystemCritical)
            .into(),
        );
    }
    let current = snapshot.features;
    let toggle = |change: fn(&mut Features, bool)| {
        context.callback(move |on| {
            let mut features = current;
            change(&mut features, on);
            Message::SetFeatures(features)
        })
    };
    let features = vec![
        switch(
            t("Files", "Fichiers"),
            t(
                "Send files to your devices, and receive theirs in Downloads",
                "Envoyer des fichiers à vos appareils, et recevoir les leurs dans Téléchargements",
            ),
            current.files,
            toggle(|features, on| features.files = on),
        ),
        switch(
            t("Links", "Liens"),
            t(
                "Links your devices share open in your browser",
                "Les liens partagés par vos appareils s'ouvrent dans votre navigateur",
            ),
            current.links,
            toggle(|features, on| features.links = on),
        ),
        switch(
            t("Ring", "Sonnerie"),
            t(
                "Ring your devices to find them, and let them ring this computer",
                "Faire sonner vos appareils pour les retrouver, et les laisser faire sonner cet ordinateur",
            ),
            current.ring,
            toggle(|features, on| features.ring = on),
        ),
        switch(
            t("Battery and network", "Batterie et réseau"),
            t(
                "Show the battery and network of your devices",
                "Afficher la batterie et le réseau de vos appareils",
            ),
            current.battery,
            toggle(|features, on| features.battery = on),
        ),
        switch(
            t("Notifications", "Notifications"),
            t(
                "Your phone's notifications show here, and you can answer them",
                "Les notifications de votre téléphone s'affichent ici, et vous pouvez y répondre",
            ),
            current.notifications,
            toggle(|features, on| features.notifications = on),
        ),
        switch(
            t("Calls", "Appels"),
            t(
                "Incoming and missed calls of your phone show here",
                "Les appels entrants et manqués de votre téléphone s'affichent ici",
            ),
            current.calls,
            toggle(|features, on| features.calls = on),
        ),
        switch(
            t("Media", "Média"),
            t(
                "Control what plays on your devices, and let them control what plays here",
                "Contrôler ce qui joue sur vos appareils, et les laisser contrôler ce qui joue ici",
            ),
            current.media,
            toggle(|features, on| features.media = on),
        ),
        switch(
            t("Volume", "Volume"),
            t(
                "Change the volume of your devices, and let them change this computer's",
                "Régler le volume de vos appareils, et les laisser régler celui de cet ordinateur",
            ),
            current.volume,
            toggle(|features, on| features.volume = on),
        ),
    ];
    let relay = card(column(
        2.0,
        vec![
            body(t("Relay", "Relais")).into(),
            secondary(snapshot.relay_url.clone())
                .font_size(12.0)
                .is_text_selection_enabled(true)
                .into(),
        ],
    ));
    let shown = SOURCE.trim_start_matches("https://");
    let source = match HyperlinkButton::new().navigate_uri(SOURCE) {
        // Its text lines up with the lines above.
        Ok(link) => link
            .margin(Thickness::new(-12.0, 4.0, 0.0, 0.0))
            .content(shown),
        Err(_) => secondary(shown).into(),
    };
    let about = card(column(
        2.0,
        vec![
            body(format!("Weft {}", env!("CARGO_PKG_VERSION"))).into(),
            secondary(t("MIT license", "Licence MIT"))
                .font_size(12.0)
                .into(),
            source,
        ],
    ));
    column(
        24.0,
        vec![
            column(
                8.0,
                vec![
                    label(t("This device", "Cet appareil")).into(),
                    device_name(weft, context),
                ],
            ),
            column(
                8.0,
                vec![
                    label(t("Sync", "Synchronisation")).into(),
                    column(4.0, sync),
                ],
            ),
            column(
                8.0,
                vec![
                    label(t("With your devices", "Avec vos appareils")).into(),
                    column(4.0, features),
                ],
            ),
            column(8.0, vec![label(t("Connection", "Connexion")).into(), relay]),
            column(8.0, vec![label(t("About", "À propos")).into(), about]),
        ],
    )
}
