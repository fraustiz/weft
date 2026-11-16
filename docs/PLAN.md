# Weft — plan de réalisation (v3)

## Index
- **Weft** synchronise le presse-papiers, les fichiers et les notifications entre Android, macOS et Windows. Un relais Internet public, chiffré de bout en bout, fait passer les données même quand le réseau isole les appareils.
- Trois apps natives : Android en Kotlin et Material 3, macOS en Swift/SwiftUI avec l'interface Apple, Windows en Rust avec WinUI 3 via windows-reactor. Un relais Docker hébergé par le mainteneur, ouvert à tous.
- Presse-papiers Android : un bouton, comme dans KDE Connect, sans Shizuku. Le sens ordinateur → téléphone est automatique.

Date : 16 novembre 2026. Statut : v3, relue et corrigée (sécurité de l'appairage, liste signée des appareils, architecture Windows, service Android selon les règles Play), rien d'implémenté ni de déployé. La v2 était un plan Android/Mac avec Shizuku : remplacée.

Nom : **Weft**, la trame en tissage. C'est le fil qui fait l'aller-retour d'un bord à l'autre et relie les fils séparés en un seul tissu. Choisi le 2026-11-16.

## 0. Contexte et décisions

- Origine : sur certains réseaux Wi-Fi (entreprise, hôtel, école), les appareils sont isolés les uns des autres. Les apps qui passent par le réseau local, comme KDE Connect, échouent alors, alors qu'une connexion sortante vers Internet passe.
- Décisions de départ (2026-11-16) :

| Sujet | Décision |
| --- | --- |
| Transport | Relais maison : image Docker hébergée par le mainteneur, **ouverte à tous les utilisateurs** |
| Presse-papiers Android → autres appareils | **Bouton**, comme dans KDE Connect ; pas de Shizuku, pas de root |
| Android | Kotlin + Jetpack Compose, **Material 3** |
| macOS | **Swift + SwiftUI**, interface Apple (HIG) |
| Windows | **Rust + WinUI 3** (windows-reactor) |
| Domaine | `weft.frohub.eu` pour l'instant |
| Nom | Weft |

## 1. Périmètre

Inventaire de KDE Connect relu dans son code Android, le 2026-11-16 : 27 plugins. Voici la correspondance avec Weft.

| Fonction (plugin KDE Connect) | Weft | Note |
| --- | --- | --- |
| Presse-papiers (`clipboard`) | **MVP** | Ordinateur → Android automatique, Android → ordinateur par bouton (§3) |
| Partage de fichiers et de liens (`share`) | **MVP** | Feuille de partage, glisser-déposer, reprise |
| Notifications Android → ordinateurs (`notifications`) | V1 | OTP masqués par Android 15 |
| Batterie (`battery`), connectivité (`connectivityreport`) | V1 | Valeurs réelles, « inconnu » sinon |
| Faire sonner le téléphone (`findmyphone`), et l'inverse (`findremotedevice`) | V1 | Commande authentifiée |
| Appels entrants (`telephony`) | V1 | Notification seulement |
| Contrôle multimédia (`mpris`, `mprisreceiver`), volume (`systemvolume`) | V2 | Selon les API de chaque système |
| SMS (`sms`), contacts (`contacts`) | Étude V2 | Permissions sensibles, règles du Play Store |
| Souris et clavier distants (`mousepad`, `remotekeyboard`…), présentation (`presenter`), commandes (`runcommand`) | Étude V2 | Consentement local, liste blanche de commandes |
| Système de fichiers distant (`sftp`), tablette graphique (`digitizer`) | Hors périmètre | — |

Les appareils sont réunis en **groupe** : jusqu'à 10 appareils par personne, n'importe quelle combinaison de plateformes (Android ↔ Mac, Android ↔ Windows, Mac ↔ Windows).

## 2. Architecture

```
Android (Kotlin) ─┐                              ┌─ macOS (Swift)
                  ├─ WSS 443 ─ relais Weft ─ WSS 443 ─┤
Windows (Rust) ───┘   (weft.frohub.eu)          └─ …
```

- **Relais** : Rust (axum + tokio, WebSocket), proposé pour partager la crate `weft-protocol` avec le client Windows. Il stocke appareils et groupes dans SQLite, et les fichiers chiffrés dans un volume à durée limitée. Il ne déchiffre jamais rien.
- **Protocole** : schémas Protobuf versionnés dans `protocol/`, générés pour Kotlin, Swift et Rust (prost). Vecteurs de test communs ; un champ de version est vérifié à la connexion.
- **Monorepo** `fraustiz/weft` :
  - `apps/android`, `apps/macos`, `apps/windows` ;
  - `relay`, `crates/weft-protocol`, `protocol` ;
  - `docs`.
- Les noms de crates publiés seront `weft-relay` et `weft-protocol` : `weft` est déjà pris sur crates.io.
- L'adresse du relais est **configurable** dans les apps. Quiconque peut héberger son propre relais avec le même `compose.yml`.

## 3. Presse-papiers

**Android → ordinateurs (bouton)**, même mécanisme que KDE Connect (`ClipboardFloatingActivity`) :
- Depuis Android 10, une app ne lit le presse-papiers que si elle a le focus.
- Le bouton ouvre une activité invisible. Elle prend le focus, lit le presse-papiers, envoie le contenu et se ferme.
- Points d'entrée :
  - action « Envoyer le presse-papiers » dans la notification ;
  - **tuile Quick Settings** ;
  - cible de la feuille de partage (« Envoyer avec Weft ») ;
  - bouton dans l'app.
- Contraintes de l'API :
  - La tuile ouvre l'activité avec `startActivityAndCollapse(PendingIntent)`. La version qui prend un `Intent` lève une exception à partir de l'API 34.
  - Si le téléphone est verrouillé, passer par `unlockAndRun()`.
  - L'action de notification est un `PendingIntent.getActivity` direct : les « trampolines » de notification sont interdits depuis Android 12.
  - Demander `POST_NOTIFICATIONS` (Android 13+).
- À tester : focus en mode multi-fenêtre, et toast système « Weft a collé » (Android 12+).

**Ordinateurs → Android (automatique)** : écrire dans le presse-papiers en arrière-plan reste permis. Android 13 et plus affiche alors sa propre confirmation de copie. `ClipDescription.EXTRA_IS_SENSITIVE` (API 33) est respecté dans les deux sens : on ne lit pas un contenu marqué sensible, et on marque ainsi ce qu'on reçoit s'il l'était.

**macOS** :
- On sonde `NSPasteboard.changeCount` toutes les ~0,5 s, puisqu'il n'existe pas de notification de changement. Lire `changeCount` et les types ne déclenche aucune alerte ; seule la lecture du contenu en déclenche une.
- L'invite de confidentialité ne s'active encore, sur macOS 26, que par une préférence (`defaults`), mais elle sera sans doute appliquée plus tard. On lit `NSPasteboard.accessBehavior` et on guide l'utilisateur vers « Toujours autoriser ».
- On ignore `org.nspasteboard.ConcealedType`, `TransientType`, `AutoGeneratedType`, et on respecte `Source`.

**Windows** :
- `AddClipboardFormatListener` (`WM_CLIPBOARDUPDATE`) sur une fenêtre `HWND_MESSAGE` dans un thread Win32 dédié (§7).
- On ignore les contenus marqués `ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore`, `CanIncludeInClipboardHistory = 0` ou `CanUploadToCloudClipboard = 0`, que posent les gestionnaires de mots de passe.
- `GetClipboardSequenceNumber` permet de reconnaître sa propre écriture.

**Règles communes** :
- texte UTF-8, 256 Kio maximum, sans troncature silencieuse ;
- identifiant unique et appareil d'origine, pour ne jamais réémettre une copie reçue ;
- à la reconnexion, ne pas écraser le presse-papiers avec une ancienne valeur ;
- pause globale et par sens ;
- aucun historique côté relais, ni par défaut sur les appareils.

## 4. Fichiers et notifications

- **Fichiers** (étape après le presse-papiers ; le MVP commence par le texte) :
  - chiffrés par morceaux sur l'appareil qui envoie, un AEAD par morceau (§6) pour permettre la reprise ;
  - envoyés en direct si le destinataire est en ligne, sinon déposés chiffrés sur le relais ;
  - reprise, intégrité vérifiée, mémoire bornée, nom de fichier assaini, jamais d'exécution automatique ;
  - supprimés du relais après réception ou au bout de 24 h.
- **Notifications** : `NotificationListenerService`, filtres par app, gestion des créations, mises à jour et suppressions. Android 15 masque les OTP sans `RECEIVE_SENSITIVE_NOTIFICATIONS`, et on ne contourne pas ce masquage. Les réponses et actions passent seulement par les actions système exposées.
  - Affichage sur Mac : `UserNotifications`.
  - Affichage sur Windows : notifications toast WinRT. Une app non empaquetée a besoin d'un AUMID et d'un raccourci ; le paquet MSIX règle ce point et le démarrage automatique.

## 5. Android : service et réveil

Les règles du Play Store réservent `connectedDevice` à un **appareil externe**, et la vidéo de déclaration doit montrer l'appairage dans l'app. Un relais Internet risque donc d'être refusé, et `remoteMessaging` ne couvre que les messages texte. D'où deux variantes :

- **Version Play Store, sans service permanent** :
  - Réveil par **FCM** en priorité haute, puis un travail court (service bref ou WorkManager) qui récupère et déchiffre le message.
  - FCM dépriorise les messages haute priorité qui n'affichent pas de notification : ceux qui arrivent du presse-papiers ou d'un fichier affichent donc une notification discrète.
  - Le socket reste ouvert seulement tant que l'app est au premier plan.
- **Version GitHub/F-Droid** : service en continu (`connectedDevice` ou `specialUse`, à trancher au prototype), et réveil par **UnifiedPush** en option. Pas de `dataSync` : durée limitée, et démarrage au boot interdit pour une cible Android 15.
- Le message FCM ne contient aucune donnée, seulement « il y a du nouveau ». Le jeton FCM est transmis à Google : c'est à écrire dans la politique de confidentialité.
- Exemption d'optimisation de batterie proposée, jamais imposée. Tests en Doze, en veille prolongée, puis après redémarrage.

## 6. Relais public multi-utilisateurs

**Principe de sécurité** : le relais est un simple transporteur, **jamais une source de confiance**. Chaque appareil tient lui-même la liste des clés du groupe. Le relais ne peut ni ajouter, ni retirer, ni remplacer un appareil.

**Identité sans compte** :
- Chaque installation génère deux paires de clés : Ed25519 pour signer, X25519 pour l'échange de clés.
  - Android Keystore ne fournit pas de clés brutes X25519/Ed25519 : on les chiffre avec une clé AES du Keystore.
  - macOS : Trousseau. Windows : DPAPI.
- Le relais connaît la **clé publique Ed25519** de chaque appareil (pour vérifier sa signature), son groupe, son jeton de réveil éventuel et sa date de dernière connexion.
- Connexion : le relais envoie un défi lié à son nom de domaine, que l'appareil signe. Pas d'e-mail, pas de mot de passe.

**Liste des appareils du groupe** :
- Chaque appareil garde **sa propre liste des appareils de confiance**.
- Un ajout ou un retrait n'est valable que s'il est **signé par un appareil déjà membre**. Le relais transporte ces messages sans pouvoir les fabriquer.
- On ne chiffre que pour les appareils de sa liste locale, jamais pour une liste fournie par le relais.

**Appairage** :
- **Avec QR** (cas normal) :
  - Le QR contient l'adresse du relais, un identifiant d'appairage, la clé publique de l'appareil qui l'affiche et un **secret à usage unique** (32 octets, 5 min). Ce secret ne passe jamais par le relais.
  - Le nouvel appareil envoie sa clé publique authentifiée par un MAC calculé avec ce secret : un faux appareil injecté par le relais est rejeté.
  - Pas de code à comparer dans ce cas.
- **Sans caméra** (entre Mac et Windows, par exemple), comparaison numérique comme en Bluetooth :
  1. Le nouvel appareil s'engage d'abord en envoyant `H(clé ‖ nonce)`.
  2. Il ne révèle sa clé et son nonce qu'après avoir reçu le nonce de l'autre.
  3. Les deux écrans affichent un code de 6 chiffres dérivé des deux clés et des deux nonces. L'utilisateur compare et confirme sur les deux appareils.
  - L'engagement empêche le relais d'essayer des millions de clés jusqu'à retomber sur le même code.
- **Perte du dernier appareil** : le groupe ne peut pas être récupéré, il faut en recréer un. C'est assumé et affiché.
- **Changer de relais** fait changer de groupe : nouvel appairage.

**Chiffrement de bout en bout** avec **libsodium** :
- Bibliothèques :
  - Android : lazysodium-android (maintenance irrégulière, à surveiller) ;
  - macOS : swift-sodium ;
  - Windows et relais : `libsodium-sys-stable` ;
  - l'interopérabilité est vérifiée par des vecteurs de test communs.
- **Messages** :
  - Clés de session par paire d'appareils avec `crypto_kx`.
  - Chiffrement **XChaCha20-Poly1305 IETF avec données associées**. L'en-tête visible du relais (émetteur, destinataire, type, identifiant, compteur) est authentifié.
  - On n'utilise pas `crypto_box_seal`, qui n'authentifie pas l'émetteur.
- **Fichiers** : clé aléatoire par fichier, un AEAD par morceau avec nonce = `idFichier ‖ index` et le drapeau « dernier morceau » dans les données associées. Cela rend la reprise possible, ce que `secretstream`, séquentiel, ne permet pas. La clé du fichier passe dans un message chiffré.
- **Anti-rejeu** : identifiant unique par message et fenêtre glissante, enregistrés sur disque, pour tolérer le désordre de la file hors ligne.
- **Limite assumée** : pas de confidentialité persistante (*forward secrecy*) en v1. Si la clé d'un appareil est volée, les messages encore stockés sur le relais restent lisibles. Pas de crypto maison.

**Ce que voit le relais** : qui parle à qui, quand, et la taille des messages. Jamais le contenu. C'est à écrire dans la politique de confidentialité.

**Abus et quotas** (valeurs de départ à ajuster) :

| Limite | Valeur |
| --- | --- |
| Enregistrement d'un appareil | Preuve de travail légère + plafond par IP |
| Identité jamais appairée | Supprimée après 24 h |
| Connexions WebSocket | Plafond par IP |
| Nombre total de groupes | Plafond global, ajustable |
| Appareils par groupe | 10 |
| Message | 256 Kio |
| Fichier | 100 Mo |
| Fichiers en attente par groupe | 1 Go, 24 h |
| Volume par groupe et par jour | 2 Go |
| Débit | Par IP et par appareil (appairages, messages) |
| Groupe inactif | Supprimé après 90 jours |

- Les fichiers ne vont qu'aux appareils appairés. Il n'y a **pas de lien public**, donc Weft ne peut pas servir d'hébergeur de fichiers.
- Les fichiers en attente sont stockés dans un volume dédié avec un plafond global. Au-delà, le relais refuse les nouveaux dépôts.

**Cadre légal** :
- **LCEN** : le mainteneur est **éditeur** du service, l'hébergeur est Oracle Cloud (région à préciser).
- **RGPD** : politique de confidentialité indiquant :
  - la base légale (exécution du service) ;
  - le sous-traitant Oracle et sa région ;
  - le transfert du jeton FCM à Google ;
  - la durée de conservation : journaux sans contenu, IP comprises, 7 jours au plus ;
  - aucune mesure d'audience.
  - Supprimer un groupe ou un appareil efface ses données sur le relais.
- **DSA** : Weft stocke des données pour ses utilisateurs, c'est donc un service d'hébergement. Il faut un point de contact (art. 11) et un mécanisme de signalement (art. 16), même si le contenu est chiffré.

**Exploitation (relais public `weft.frohub.eu`)** :
- image multi-architecture sur `ghcr.io/fraustiz/weft/relay`, comme les autres projets du mainteneur ;
- proxy inverse avec WebSocket, `weft.frohub.eu` ;
- limite de mémoire et de CPU du conteneur, le serveur étant partagé avec d'autres services ;
- healthcheck, supervision et sauvegarde du SQLite ;
- montée de version : le protocole garde la compatibilité avec la version précédente, et les clients trop anciens reçoivent une erreur explicite.

## 7. Interfaces

Chaque app suit **le système de design de sa plateforme**. Unison sert de référence pour l'organisation et les parcours, pas pour imposer un style.

- **Android, Material 3** (Compose) :
  - schéma de couleurs, formes et typographie Material ;
  - accent tiré des relevés d'Unison ;
  - couleurs dynamiques (Material You) proposées en option ;
  - composants standards : `NavigationBar`, cartes, `FilledButton`, tuile Quick Settings.
- **macOS, interface Apple** (SwiftUI, HIG) :
  - `MenuBarExtra` pour l'accès rapide, fenêtre `Settings` ;
  - app d'accessoire (`LSUIElement`), ouverture à la connexion avec `SMAppService`, runtime renforcé pour la notarisation ;
  - SF Symbols, styles de liste et de formulaire natifs, typographie système ;
- **Windows, WinUI 3 / Fluent** (windows-reactor, version **0.100.0 figée**) :
  - `NavigationView`, fond Mica, pour la fenêtre principale ;
  - **ce que windows-reactor 0.100.0 ne gère pas** (lu dans son code par la relecture) :
    - fenêtre cachée ;
    - accès au HWND ;
    - zone de notification ;
    - notifications toast ;
    - minuteur ;
    - fermeture de la dernière fenêtre : elle arrête `App::run`.
  - **Architecture retenue** : un **thread Win32 dédié** (windows-rs) avec une fenêtre `HWND_MESSAGE`. Il gère :
    - `AddClipboardFormatListener` ;
    - l'icône de la zone de notification (`Shell_NotifyIcon`) ;
    - son menu (`TrackPopupMenu`, menu Win32 et non Fluent) ;
    - la connexion au relais.

    La fenêtre WinUI n'est qu'une vue, ouverte et fermée à la demande. Pont vers l'interface : une tâche de fond relancée à chaque message (`LocalSender` n'est pas `Send`).
  - **Question pour le prototype** : `App::run` peut-il être relancé dans le même processus ? Sinon, la fenêtre devient un processus séparé.
  - Windows App Runtime 2 requis, en dépendance de framework ou en mode autonome.
  - Pièges déjà connus : `Expander`, `FontIcon`.
- **Référence visuelle Unison** : captures d'écran des apps Mac et Android, prises sur nos appareils. On **n'extrait pas** les ressources de son APK : la relecture signale un risque de licence et de droit d'auteur, et des captures suffisent.
  - **rea** (REA) sert seulement à **comprendre des comportements** quand c'est utile : par exemple, comment Unison gère son appairage ou son service Android. On n'en tire que des faits, jamais de code ni de ressources.
  - Nos propres tokens vont dans `docs/design/`.
- **Passe anti-« générique »** selon la méthode unslop (https://github.com/heydamir/unslop) : il vise le web, on l'adapte au natif. D'abord une direction, puis une passe par couche : espacement, couleur, icônes, textes, typographie, détails, mouvement. En cas de conflit, le système de la plateforme l'emporte.
- **Écrans communs** :
  1. Appairage (QR + code).
  2. Appareils du groupe et leur état.
  3. Presse-papiers (dernier envoi, sans afficher de contenu sensible).
  4. Transferts.
  5. Notifications (sur les ordinateurs).
  6. Réglages, avec relais personnalisé et diagnostics.
- Chaque état d'erreur est distinct et propose une action : relais injoignable, appareil hors ligne, quota atteint, version trop ancienne, permission manquante.
- **Accessibilité** : TalkBack, VoiceOver, Narrateur, navigation au clavier, grandes polices, contraste, clair/sombre, réduction des animations.

## 8. Étapes et critères de sortie

| Étape | Livrable | Condition pour continuer |
| --- | --- | --- |
| 0. Prototypes | (a) bouton Android : tuile + activité invisible sur un vrai téléphone Android, verrouillé compris ; (b) Windows : thread Win32 (presse-papiers + zone de notification) + fenêtre windows-reactor ouverte et refermée deux fois ; (c) libsodium interopérable entre Kotlin, Swift et Rust (vecteurs `crypto_kx` + XChaCha20-Poly1305) ; (d) réveil UnifiedPush sur un vrai téléphone | Les quatre marchent sur de vrais appareils ; sinon, décision avec le mainteneur |
| 1. Protocole + relais | `weft-protocol`, relais local Docker, identité, appairage QR et code, liste signée des appareils, quotas | Faux appareil injecté par le relais, rejeu et altération rejetés ; tests de quotas et d'abus |
| 2. Presse-papiers (MVP) | Les 3 clients, texte seulement | 100 copies par sens et par paire, accents/emojis/multilignes, aucune boucle, < 2 s hors réveil |
| 3. Référence visuelle | Captures d'Unison, tokens Weft, un écran clé par plateforme | Direction validée par le mainteneur, passe unslop |
| 4. Fichiers | Transferts directs et différés | Petits/gros fichiers, reprise après coupure, intégrité, nettoyage à 24 h |
| 5. Relais en ligne | `weft.frohub.eu`, mentions légales, politique de confidentialité, contact et signalement DSA, supervision | Deux réseaux réels, dont un Wi-Fi qui isole les appareils ; limites mémoire respectées |
| 6. Notifications + utilitaires | Notifications, batterie, sonnerie, appels | Permissions réelles, OTP non exposés |
| 7. Distribution | APK signé (GitHub Releases, puis Play Store), app Mac signée et notariée, app Windows signée (MSIX ou installeur) | Installation, mise à jour, révocation, essai de plusieurs jours |
| 8. V2 | Multimédia, puis étude SMS/entrée distante | Sous-plan par fonction |

Les estimations viendront après l'étape 0.

## 9. Coûts et comptes à prévoir

| Poste | Coût |
| --- | --- |
| Google Play Console | 25 $, une fois |
| Apple Developer Program | 99 $ par an, nécessaire pour signer et notarier l'app Mac distribuée à tous (et pour le Mac App Store) |
| Signature Windows | Azure Trusted Signing (abonnement mensuel), sinon avertissement SmartScreen ; Microsoft Store : compte développeur |
| Hébergement | Serveur existant ; surveiller la bande passante sortante |

## 10. Décisions encore ouvertes

1. Langage du relais : Rust (proposé) ou TypeScript.
2. ~~FCM~~ : sans objet, pas de Play Store pour l'instant (§13).
3. ~~Licence~~ : MIT, 100 % open source (§13).
4. Valeurs des quotas (§6).
5. Comptes Apple et signature Windows : reportés tant qu'on ne publie pas sur les stores (§13).

## 11. Sources

- Code de KDE Connect Android : https://invent.kde.org/network/kdeconnect-android (`plugins/clipboard/ClipboardFloatingActivity.kt`, `AndroidManifest.xml`)
- Presse-papiers Android : https://developer.android.com/about/versions/10/privacy/changes
- Types de services en continu : https://developer.android.com/develop/background-work/services/fgs/service-types
- Notifications sensibles d'Android 15 : https://developer.android.com/about/versions/15/behavior-changes-all
- Changements d'Android 14 (tuiles, lancement d'activité) : https://developer.android.com/about/versions/14/behavior-changes-14
- Règles Play sur les services en continu : https://support.google.com/googleplay/android-developer/answer/13392821
- `NSPasteboard.accessBehavior` : https://developer.apple.com/documentation/appkit/nspasteboard/accessbehavior-swift.enum
- DSA (règlement UE 2022/2065), articles 11 et 16 : https://eur-lex.europa.eu/eli/reg/2022/2065/oj
- `changeCount` : https://developer.apple.com/documentation/appkit/nspasteboard/changecount
- Presse-papiers Windows (formats d'exclusion) : https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats
- libsodium : https://doc.libsodium.org/
- windows-reactor : https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor
- rea : https://github.com/morluto/rea · unslop : https://github.com/heydamir/unslop

## 12. Décisions du 2026-11-16 (après la v3)

- **Aucune publication sur les stores pour l'instant.** Distribution par GitHub Releases :
  - APK signé (F-Droid possible plus tard) ;
  - app Mac et app Windows.
- **Conséquences** :
  - Android : seulement la variante sans Play Store de §5, c'est-à-dire un service en continu et un réveil UnifiedPush en option. **FCM n'est plus nécessaire**, donc pas de jeton envoyé à Google.
  - macOS : sans compte Apple Developer, l'app n'est ni signée ni notariée. Il faut l'ouvrir par clic droit → Ouvrir, ou depuis Réglages → Confidentialité. On l'indique dans le README.
  - Windows : sans signature, SmartScreen affiche un avertissement au premier lancement.
- **Licence MIT, projet 100 % open source** : apps, relais, protocole, logo, documentation. Dépôt public `fraustiz/weft`.
- **Logo** : uniquement en **SVG dessiné à la main**, aucune image générée par IA. Il est bicolore, orange coucher de soleil `#F26A21` sur encre `#17130F` (ou sur crème `#F6EFE6`). « Coucher de soleil » désigne la couleur, pas un soleil dessiné. Le thème : une aiguille qui coud du numérique (pixels, lignes de trame, appareils). Six pistes sont dans `brand/variants/`, la page de comparaison est `brand/showcase.html`. 
- **Logo retenu (2026-11-16)** : piste **2B « Gros pixels »**, trois gros pixels arrondis traversés par une aiguille, qui sort par le pixel manquant. Jeu final dans `brand/final/` : symbole, icônes sombre et claire, favicon, symbole + mot « weft » dessiné au trait. Charte dans `brand/README.md`. Les PNG, `.icns`, `.ico` et icônes adaptatives se génèrent depuis les SVG au build.
