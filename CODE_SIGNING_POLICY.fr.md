# Politique de signature (Code signing policy)

[English](CODE_SIGNING_POLICY.md)

Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).
(Signature de code gratuite fournie par SignPath.io, certificat de la SignPath Foundation.)

> **État :** Weft demande à rejoindre le programme gratuit de signature de code de la SignPath Foundation pour les projets open source. Tant que le projet n'est pas accepté, l'exécutable Windows est publié sans signature et SmartScreen affiche un avertissement au premier lancement. Cette politique décrit comment les versions sont signées une fois le projet accepté.

## Ce qui est signé

La SignPath Foundation signe un fichier par version : l'app Windows, `weft-X.Y.Z-windows.exe`.

- Il est compilé à partir du code source de ce dépôt ([github.com/fraustiz/weft](https://github.com/fraustiz/weft), licence MIT) et de ses dépendances open source, sur des machines hébergées par GitHub, par le workflow [`.github/workflows/app-release.yml`](.github/workflows/app-release.yml), quand un tag de version `app-vX.Y.Z` est poussé.
- Le workflow soumet l'exécutable à SignPath, et un approbateur (voir plus bas) approuve à la main chaque demande de signature avant qu'elle soit signée. Les exécutions manuelles du workflow (essais à blanc) ne soumettent jamais de demande de signature.
- L'exécutable signé est celui publié sur [GitHub Releases](https://github.com/fraustiz/weft/releases), et celui dont l'empreinte SHA-256 figure dans `windows-update.json`, que les copies installées vérifient avant de se mettre à jour.
- Ses propriétés de fichier nomment le produit `Weft`, avec le numéro de version.

Le certificat ne sert à rien d'autre : aucun logiciel tiers, aucune compilation faite hors de ce workflow.

Les autres apps sont signées sans SignPath :

- **macOS :** `Weft-X.Y.Z-mac.zip` est signée ad hoc et non notariée. Ses mises à jour sont vérifiées par [Sparkle](https://sparkle-project.org) avec une signature EdDSA, d'après [`macos/sparkle-public-key.txt`](macos/sparkle-public-key.txt).
- **Android :** `weft-X.Y.Z.apk` est signée avec la clé de publication du projet.
- **iOS :** `Weft-X.Y.Z-ios.ipa` n'a qu'une signature ad hoc ; les testeurs la re-signent avec leur propre identifiant Apple en l'installant.

## Rôles de l'équipe

| Rôle | Membres |
| --- | --- |
| Committers and reviewers (contributeurs et relecteurs) | [fraustiz](https://github.com/fraustiz) |
| Approvers (approbateurs) | [fraustiz](https://github.com/fraustiz) |

- Les **contributeurs** (committers) peuvent pousser dans le dépôt. Les modifications de toute autre personne sont relues par un contributeur avant d'être fusionnées.
- Les **relecteurs** (reviewers) relisent les contributions des personnes extérieures à l'équipe.
- Les **approbateurs** (approvers) approuvent chaque demande de signature dans SignPath, après avoir vérifié qu'elle vient d'un tag de version de ce dépôt.

Tous les membres de l'équipe doivent utiliser l'authentification multifacteur (MFA) pour leur compte GitHub et leur compte SignPath.

## Confidentialité

Weft transfère des données entre vos propres appareils : c'est son but, il utilise donc le réseau. Il ne contient ni télémétrie, ni statistiques d'usage, ni rapports de plantage, ni publicité, et n'envoie rien à d'autres systèmes que ceux-ci.

- **Relais.** Les appareils appairés échangent leurs messages par un relais WebSocket. Par défaut, c'est le relais public `wss://weft.frohub.eu/v1`, géré par le projet ; vous pouvez héberger le vôtre ([`relay/README.md`](relay/README.md)). Tout ce que vous envoyez (presse-papiers, fichiers, notifications, noms d'appareils, autres fonctions) est chiffré de bout en bout sur votre appareil : le relais ne peut pas le lire. Le relais voit en revanche des métadonnées : votre adresse IP, les identifiants de vos appareils (dérivés de leurs clés publiques) et leur groupe, et quand et combien chaque appareil envoie à quel autre. Il enregistre une ligne par appareil (son identifiant et son numéro de groupe), supprimée quand l'appareil quitte son groupe. Quand un membre du groupe est hors ligne, il garde pour lui les messages chiffrés de changement de groupe et de nom d'appareil jusqu'à leur livraison, au plus 30 jours et 64 messages par appareil. Rien d'autre, fichiers compris, n'est jamais conservé : c'est abandonné si le destinataire est hors ligne. Ses journaux ne contiennent jamais le contenu des messages.
- **Réseau local.** Quand des appareils appairés partagent un réseau local, ils s'échangent leurs adresses IP privées (chiffrées de bout en bout, par le relais) et se connectent directement. Chaque appareil écoute sur un port TCP local et ne parle qu'aux appareils avec lesquels il est appairé.
- **Mises à jour.** Les apps macOS, Android et Windows consultent [GitHub Releases](https://github.com/fraustiz/weft/releases) au plus une fois par jour pour savoir s'il existe une nouvelle version, et ne la téléchargent que si vous choisissez de l'installer. La vérification automatique se désactive dans les réglages de chaque app. Ces requêtes vont à GitHub, sous la [déclaration de confidentialité de GitHub](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement). L'app iOS ne cherche pas de mises à jour.

Le site [weft.frohub.eu](https://weft.frohub.eu) mesure son audience avec [Umami](https://umami.is), hébergé sur le propre serveur du responsable du projet : aucun cookie, uniquement des statistiques anonymes et agrégées, rien n'est partagé avec des tiers.

## Désinstaller

Avant de désinstaller, vous pouvez retirer l'appareil de votre groupe (dans Weft, sur cet appareil ou sur un autre) : le relais supprime alors sa ligne.

### Windows

1. Dans les réglages de Weft, désactivez **Démarrer avec Windows**. Cela supprime la valeur `Weft` sous `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
2. Quittez Weft : clic droit sur son icône dans la zone de notification, puis **Quitter**.
3. Supprimez `weft-windows.exe` (et `weft-windows.exe.old` ou `weft-windows.exe.new` à côté, si une mise à jour en a laissé un).
4. Supprimez le dossier `%APPDATA%\weft` (ou celui défini par `WEFT_STATE`) : il contient les clés de cet appareil, les appareils appairés et les réglages.
5. Supprimez la clé de registre `HKEY_CURRENT_USER\Software\Classes\AppUserModelId\Weft.Windows`, qui donne leur nom et leur icône aux notifications de Weft.

La même chose dans PowerShell, une fois Weft quitté :

```powershell
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Weft -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "$env:APPDATA\weft"
Remove-Item -Recurse -Force 'HKCU:\Software\Classes\AppUserModelId\Weft.Windows'
```

Si vous avez autorisé Weft dans le Pare-feu Windows Defender, vous pouvez aussi le retirer des applications autorisées. Les fichiers reçus restent dans votre dossier Téléchargements.

### macOS

1. Dans les réglages de Weft, désactivez **Ouvrir à l’ouverture de session**.
2. Quittez Weft depuis son icône dans la barre des menus (**Quitter Weft**).
3. Mettez `Weft.app` à la corbeille.
4. Supprimez `~/Library/Application Support/Weft` et, s'ils existent, `~/Library/Preferences/fr.frohub.weft.mac.plist` et `~/Library/Caches/fr.frohub.weft.mac`.

### Android

Désinstallez Weft comme n'importe quelle app (appui long sur son icône, puis **Désinstaller**). Android supprime ses données avec elle ; les fichiers reçus restent dans Téléchargements.
