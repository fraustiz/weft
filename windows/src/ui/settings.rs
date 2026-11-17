//! The settings page: this device's name, the sync switches, the relay and
//! what the app is.

use windows_reactor::*;

use super::app::{CommandDraft, Message, Weft};
use super::widgets::{body, button, card, column, label, primary_button, row, secondary};
use super::words;
use crate::features::Features;
use crate::i18n::{is_french, t};
use crate::update::{INSTALLED_VERSION_NAME, UpdateError, UpdateState};

/// The repository, shown in About.
const SOURCE: &str = "https://github.com/fraustiz/weft";

/// A switch with its title and what it does, as Windows Settings shows
/// them. The On and Off words are given, or they would follow the system
/// language rather than the app's.
pub(super) fn switch(title: &str, caption: &str, on: bool, toggled: Callback<bool>) -> View {
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

/// An icon button with its name as a tooltip, for a row's actions.
fn icon_button(symbol: Symbol, name: &str, on_click: Callback<()>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .vertical_alignment(VerticalAlignment::Center)
        .automation_name(name)
        .on_click(on_click)
        .content(SymbolIcon::new().symbol(symbol))
        .tooltip(name)
}

/// The form of a command being added or changed.
fn command_editor(draft: &CommandDraft, context: &ViewContext<Weft>) -> View {
    let ready = !draft.name.trim().is_empty() && !draft.line.trim().is_empty();
    let save = if draft.id.is_some() {
        t("Save", "Enregistrer")
    } else {
        t("Add", "Ajouter")
    };
    let mut actions = vec![button(
        t("Cancel", "Annuler"),
        context.message(Message::CancelCommand),
    )];
    if ready {
        actions.insert(
            0,
            primary_button(save, context.message(Message::SaveCommand)),
        );
    }
    card(column(
        8.0,
        vec![
            TextBox::new()
                .text(draft.name.clone())
                .placeholder_text(t(
                    "Name, as your devices show it",
                    "Nom, tel que vos appareils l'affichent",
                ))
                .automation_name(t("Shortcut name", "Nom du raccourci"))
                .on_text_changed(context.callback(Message::CommandNameEdited))
                .into(),
            TextBox::new()
                .text(draft.line.clone())
                .placeholder_text(t(
                    "Command line, run by cmd /C",
                    "Ligne de commande, lancée par cmd /C",
                ))
                .automation_name(t("Command line", "Ligne de commande"))
                .on_text_changed(context.callback(Message::CommandLineEdited))
                .into(),
            row(actions),
        ],
    ))
}

/// The ready-made shortcuts not added yet, two by two: each fills the
/// form of a new one.
fn presets(weft: &Weft, context: &ViewContext<Weft>) -> Option<View> {
    let added = |line: &str| {
        weft.snapshot
            .commands
            .iter()
            .any(|known| known.line == line)
    };
    let buttons: Vec<KeyedView> = words::command_presets()
        .iter()
        .enumerate()
        .filter(|(_, (_, line))| !added(line))
        .enumerate()
        .map(|(place, (index, (name, _)))| {
            let cell = Button::new()
                .grid_row((place / 2) as i32)
                .grid_column((place % 2) as i32)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .horizontal_content_alignment(HorizontalAlignment::Left)
                .automation_name(*name)
                .on_click(context.message(Message::UsePreset(index)))
                .content(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            SymbolIcon::new().symbol(Symbol::Add).opacity(0.7),
                            TextBlock::new()
                                .text(*name)
                                .max_lines(1)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                        )),
                );
            KeyedView::new(index, cell)
        })
        .collect();
    if buttons.is_empty() {
        return None;
    }
    let rows = vec![GridLength::Auto; buttons.len().div_ceil(2)];
    Some(card(column(
        8.0,
        vec![
            secondary(t("Ready-made", "Prêts à l'emploi"))
                .font_size(12.0)
                .into(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::STAR])
                .rows(rows)
                .column_spacing(8.0)
                .row_spacing(8.0)
                .keyed_children(buttons),
        ],
    )))
}

/// The shortcuts the devices may run, each with Edit and Remove, then the
/// form, or the Add button and the ready-made ones.
fn commands(weft: &Weft, context: &ViewContext<Weft>) -> View {
    let editing = weft.command_draft.as_ref();
    let mut views: Vec<View> = vec![
        secondary(t(
            "Buttons on your phone that run something on this PC, such as locking it. Only the name goes to the phone, never the command line.",
            "Des boutons sur votre téléphone qui lancent une action sur ce PC, comme le verrouiller. Seul le nom part vers le téléphone, jamais la ligne de commande.",
        ))
        .font_size(12.0)
        .into(),
    ];
    for command in &weft.snapshot.commands {
        if editing.is_some_and(|draft| draft.id.as_deref() == Some(command.id.as_str())) {
            views.push(command_editor(editing.unwrap(), context));
            continue;
        }
        let (edit, remove) = if crate::i18n::is_french() {
            (
                format!("Modifier {}", command.name),
                format!("Supprimer {}", command.name),
            )
        } else {
            (
                format!("Edit {}", command.name),
                format!("Remove {}", command.name),
            )
        };
        views.push(card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto, GridLength::Auto])
                .column_spacing(4.0)
                .children((
                    StackPanel::new()
                        .grid_column(0)
                        .spacing(2.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            body(command.name.clone()),
                            secondary(command.line.clone())
                                .font_size(12.0)
                                .max_lines(1)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                        )),
                    Border::new().grid_column(1).content(icon_button(
                        Symbol::Edit,
                        &edit,
                        context.message(Message::EditCommand(command.id.clone())),
                    )),
                    Border::new().grid_column(2).content(icon_button(
                        Symbol::Delete,
                        &remove,
                        context.message(Message::RemoveCommand(command.id.clone())),
                    )),
                )),
        ));
    }
    match editing {
        Some(draft) if draft.id.is_none() => views.push(command_editor(draft, context)),
        Some(_) => {}
        None => {
            views.push(
                Border::new()
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .content(button(
                        t("Add shortcut", "Ajouter un raccourci"),
                        context.message(Message::AddCommand),
                    )),
            );
            views.extend(presets(weft, context));
        }
    }
    column(4.0, views)
}

fn update_error_text(error: &UpdateError) -> String {
    match error {
        UpdateError::Network(_) => t(
            "Could not reach GitHub. Check the network, then try again.",
            "Impossible de joindre GitHub. Vérifiez le réseau, puis réessayez.",
        )
        .to_owned(),
        UpdateError::NoRelease => t(
            "No Windows release is published yet.",
            "Aucune version Windows n'est encore publiée.",
        )
        .to_owned(),
        UpdateError::Http(code) => {
            if is_french() {
                format!("GitHub a répondu avec le code HTTP {code}. Réessayez plus tard.")
            } else {
                format!("GitHub answered with HTTP {code}. Try again later.")
            }
        }
        UpdateError::Invalid(_) => t(
            "The update information is not valid. Nothing was installed.",
            "Les informations de mise à jour ne sont pas valides. Rien n'a été installé.",
        )
        .to_owned(),
        UpdateError::BadHash => t(
            "The downloaded file failed checksum verification. Nothing was installed.",
            "Le fichier téléchargé a échoué à la vérification d'intégrité. Rien n'a été installé.",
        )
        .to_owned(),
        UpdateError::WriteProtected => t(
            "Weft cannot replace itself in this folder. Download the update from GitHub instead.",
            "Weft ne peut pas se remplacer dans ce dossier. Téléchargez plutôt la mise à jour sur GitHub.",
        )
        .to_owned(),
        UpdateError::Install(_) => t(
            "The update could not be installed. Nothing was changed.",
            "La mise à jour n'a pas pu être installée. Rien n'a été modifié.",
        )
        .to_owned(),
    }
}

fn update_status_line(state: &UpdateState) -> Option<(String, bool)> {
    let french = is_french();
    let text = match state {
        UpdateState::Idle => return None,
        UpdateState::Checking => {
            t("Checking for updates…", "Recherche de mises à jour…").to_owned()
        }
        UpdateState::UpToDate => t("Weft is up to date", "Weft est à jour").to_owned(),
        UpdateState::Available(info) => {
            if french {
                format!("Version {} disponible", info.version_name)
            } else {
                format!("Version {} available", info.version_name)
            }
        }
        UpdateState::Downloading { info, done, total } => match total.filter(|&len| len > 0) {
            Some(total) => {
                let percent = (done.saturating_mul(100) / total).min(100);
                let (done_str, total_str) = (words::size(*done), words::size(total));
                if french {
                    format!(
                        "Téléchargement de {} — {done_str} sur {total_str} ({percent}\u{a0}%)",
                        info.version_name
                    )
                } else {
                    format!(
                        "Downloading {} — {done_str} of {total_str} ({percent}%)",
                        info.version_name
                    )
                }
            }
            None => {
                if french {
                    format!("Téléchargement de {}…", info.version_name)
                } else {
                    format!("Downloading {}…", info.version_name)
                }
            }
        },
        UpdateState::Installing(info) => {
            if french {
                format!("Installation de {}…", info.version_name)
            } else {
                format!("Installing {}…", info.version_name)
            }
        }
        UpdateState::Restarting(info) => {
            if french {
                format!("Redémarrage vers {}…", info.version_name)
            } else {
                format!("Restarting into {}…", info.version_name)
            }
        }
        UpdateState::Failed { error, .. } => return Some((update_error_text(error), true)),
    };
    Some((text, false))
}

fn updates(weft: &Weft, context: &ViewContext<Weft>) -> View {
    let state = &weft.updates.state;
    let busy = matches!(
        state,
        UpdateState::Checking
            | UpdateState::Downloading { .. }
            | UpdateState::Installing(_)
            | UpdateState::Restarting(_)
    );

    let mut header = vec![body(format!("Weft {INSTALLED_VERSION_NAME}")).into()];
    if let Some((text, critical)) = update_status_line(state) {
        let line = if critical {
            body(text).foreground(ThemeBrush::SystemCritical)
        } else {
            secondary(text).font_size(12.0)
        };
        header.push(line.into());
    }

    let mut card_views = vec![column(2.0, header)];

    match state {
        UpdateState::Downloading { done, total, .. } => {
            let bar = match total {
                Some(total) if *total > 0 && *done > 0 => ProgressBar::new()
                    .maximum(*total as f64)
                    .value((*done).min(*total) as f64),
                _ => ProgressBar::new().is_indeterminate(true),
            };
            card_views.push(bar.into());
        }
        UpdateState::Installing(_) | UpdateState::Restarting(_) => {
            card_views.push(ProgressBar::new().is_indeterminate(true).into());
        }
        _ => {}
    }

    let mut actions = Vec::new();
    match state {
        UpdateState::Failed {
            error: UpdateError::WriteProtected,
            ..
        } => actions.push(primary_button(
            t("Open GitHub Releases", "Ouvrir GitHub Releases"),
            context.message(Message::OpenReleasesPage),
        )),
        UpdateState::Available(_) | UpdateState::Failed { info: Some(_), .. } => {
            actions.push(primary_button(
                t("Install and restart", "Installer et redémarrer"),
                context.message(Message::InstallUpdate),
            ))
        }
        _ => {}
    }
    actions.push(
        Button::new()
            .is_enabled(!busy)
            .on_click(context.message(Message::CheckUpdates))
            .content(t("Check for updates", "Vérifier les mises à jour")),
    );
    card_views.push(row(actions));

    column(
        4.0,
        vec![
            card(column(8.0, card_views)),
            switch(
                t("Check automatically", "Vérifier automatiquement"),
                t(
                    "At most once a day, when Weft starts",
                    "Au plus une fois par jour, au démarrage de Weft",
                ),
                weft.updates.auto_check,
                context.callback(Message::SetAutoUpdate),
            ),
        ],
    )
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
        switch(
            t("Messages", "Messages"),
            t(
                "Read and send your phone's SMS here. The phone asks for its own permission first",
                "Lire et envoyer ici les SMS de votre téléphone. Le téléphone demande d'abord sa propre autorisation",
            ),
            current.sms,
            toggle(|features, on| features.sms = on),
        ),
        switch(
            t("Shortcuts", "Raccourcis"),
            t(
                "Your phone can run the shortcuts listed below",
                "Votre téléphone peut lancer les raccourcis ci-dessous",
            ),
            current.commands,
            toggle(|features, on| features.commands = on),
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
            body(format!("Weft {INSTALLED_VERSION_NAME}")).into(),
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
            column(
                8.0,
                vec![
                    label(t(
                        "Shortcuts for your phone",
                        "Raccourcis pour le téléphone",
                    ))
                    .into(),
                    commands(weft, context),
                ],
            ),
            column(8.0, vec![label(t("Connection", "Connexion")).into(), relay]),
            column(
                8.0,
                vec![
                    label(t("Updates", "Mises à jour")).into(),
                    updates(weft, context),
                ],
            ),
            column(8.0, vec![label(t("About", "À propos")).into(), about]),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::UpdateInfo;

    #[test]
    fn update_status_and_errors_are_distinct_and_calm() {
        assert_eq!(update_status_line(&UpdateState::Idle), None);
        let info = UpdateInfo {
            version_code: 10,
            version_name: "0.2.0".into(),
            url: "https://github.com/fraustiz/weft/releases/download/app-v0.2.0/weft-0.2.0-windows.exe".into(),
            sha256: "a".repeat(64),
        };
        let (available, critical) =
            update_status_line(&UpdateState::Available(info.clone())).unwrap();
        assert!(!critical);
        assert!(available.contains("0.2.0"));

        let (downloading, critical) = update_status_line(&UpdateState::Downloading {
            info,
            done: 512 * 1024,
            total: Some(1024 * 1024),
        })
        .unwrap();
        assert!(!critical);
        assert!(downloading.contains("50"));

        let errors = [
            UpdateError::Network("timeout".into()),
            UpdateError::NoRelease,
            UpdateError::Http(503),
            UpdateError::Invalid("bad".into()),
            UpdateError::BadHash,
            UpdateError::WriteProtected,
            UpdateError::Install("io".into()),
        ];
        let mut seen = Vec::new();
        for error in &errors {
            let text = update_error_text(error);
            assert!(text.ends_with('.'), "{text}");
            assert!(!text.contains('!'), "{text}");
            assert!(!seen.contains(&text), "{text}");
            seen.push(text);
        }
    }
}
