# Notes de version

[English](CHANGELOG.md) · **Français**

Ce projet suit le [versionnage sémantique](https://semver.org/lang/fr/). Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/).

## [Non publié]

## [0.4.0] - 2026-11-04

### Ajouté

- Paquets installables et portables : un installeur `setup.exe` pour Windows, un `.dmg` pour macOS et un `.deb` pour Debian et Ubuntu, à côté des archives portables. Un fichier `portable.txt` à côté de l'exécutable range la configuration, les journaux et les scripts dans un dossier `data` voisin.
- Vérification des mises à jour : une fois par jour au démarrage (ou avec *Vérifier maintenant* dans Réglages › Confidentialité), ircnext demande à GitHub la dernière version. Un bandeau propose de l'installer : l'installeur est téléchargé, vérifié avec les sommes SHA-256 de la release, puis ouvert. Les copies portables ouvrent la page de la release.
- Confirmation avant de coller plusieurs lignes : à partir de 3 lignes (réglable dans Réglages › Discussion, 0 pour désactiver), une fenêtre montre ce qui sera envoyé, un message par ligne, et propose de l'envoyer en une seule ligne ou d'annuler. Les collages plus courts gardent leurs lignes, séparées par des espaces, au lieu d'être collées bout à bout.
- Panneau des liens, comme le « URL catcher » de mIRC : `/links` ou ⌘/Ctrl+Maj+L liste les liens web postés dans les onglets ouverts, du plus récent au plus ancien, avec leur auteur et leur salon. On peut les filtrer, les ouvrir, les copier ou revenir à la conversation.
- Ligne « Nouveaux messages » : en revenant sur un onglet, une ligne rouge montre où commencent les messages écrits pendant votre absence.
- Liste de notification, comme dans mIRC : `/notify alice` (ou le formulaire du réseau) signale dans l'onglet du serveur quand quelqu'un se connecte ou part, avec une notification à l'arrivée. `/notify` montre qui est en ligne, `/notify -r alice` retire. S'appuie sur `MONITOR` (IRCv3). Les scripts reçoivent les événements `on NOTIFY` et `on UNOTIFY`.
- Liste des salons : `/list` ouvre une fenêtre avec les salons du réseau, les plus peuplés d'abord, avec leur sujet et leur nombre d'utilisateurs. On cherche par nom ou par sujet, et on rejoint en un clic.
- Réglages du salon (le « channel central » de mIRC) : le bouton ⚙ d'un salon, ou `/channel`, ouvre son sujet, ses modes (pas de messages extérieurs, sujet protégé, sur invitation, modéré, secret, clé, limite d'utilisateurs) et ses listes de bannissements, d'exceptions et d'invitations. Les opérateurs les modifient sur place ; les autres les voient en lecture seule.
- Listes d'auto-op et d'auto-voice, comme dans mIRC : `/aop *!*@hote.fiable` ou `/avoice pseudo #salon` donnent `+o` ou `+v` aux personnes correspondantes quand elles rejoignent un salon que vous gérez. `/aop` affiche la liste, `/aop -r` retire. Propres à chaque réseau.
- Visionneuse de journaux : `/logs`, ou *Parcourir* à côté du dossier des journaux dans Réglages › Discussion, liste les conversations enregistrées par réseau et les affiche avec une recherche, en s'ouvrant sur la conversation en cours.
- Les liens `irc://` et `ircs://` sont cliquables : sur un réseau connu, ils rejoignent le salon (en se connectant d'abord si besoin), sinon le formulaire du réseau s'ouvre prérempli. Ils apparaissent aussi dans le panneau des liens.
- SASL SCRAM-SHA-256 : quand le serveur le propose, le mot de passe du compte est prouvé sans être envoyé, et le serveur doit prouver qu'il le connaît aussi. Si le serveur le refuse (mot de passe stocké dans un ancien format), PLAIN est essayé une fois, sur TLS.
- Confiance au premier usage pour les certificats non vérifiés : avec *Accepter les certificats invalides*, l'empreinte SHA-256 du premier certificat est retenue, et un certificat différent est ensuite refusé (interception possible). Le formulaire du réseau l'affiche et permet de l'oublier.

### Corrigé

- Scripts : les gestionnaires `on QUIT` et `on NICK` n'ont pas de champ de cible, comme dans mIRC (`on *:QUIT:commandes`).

## [0.3.0] - 2026-11-04

### Ajouté

- Gestionnaire de scripts dans *Réglages → Scripts* : chaque fichier avec son état (chargé avec ses alias et événements, désactivé, ou son erreur), un interrupteur pour l'activer ou le désactiver, et des boutons pour recharger, ouvrir le dossier ou modifier un fichier.
- Minuteries de scripts (`/timer`, `/timers`), tables de hachage (`hadd`, `$hget`…), fichiers ini (`$readini`, `/writeini`) et texte (`$read`, `/write`), limités au dossier `scripts` ; variables globales conservées d'une session à l'autre ; commande `/echo`.
- Événements de scripts : `on TEXT`, `ACTION`, `NOTICE`, `JOIN`, `PART`, `KICK`, `QUIT`, `NICK`, `TOPIC`, `INVITE`, `OP`, `DEOP`, `VOICE`, `DEVOICE`, `CONNECT`, `DISCONNECT`, `START` et `INPUT` (qui peut retenir un texte tapé), avec les textes filtrés, cibles et préfixes `me:`/`!`/`@` de mIRC.
- Scripts mIRC (mSL), première partie : les fichiers `.mrc` du dossier `scripts` sont chargés au démarrage et avec `/reload` ; les alias se tapent comme des commandes ou servent d'identifiants, avec variables, `if`/`elseif`/`else`, `while`, conditions mIRC et une cinquantaine d'identifiants. Un script qui boucle sans fin est arrêté.
- Chiffrement FiSH, compatible FiSH 10 : `/setkey [cible] <clé>` (préfixe `cbc:` pour CBC), `/delkey`, et `/keyx <pseudo>` pour un échange de clés DH1080. Messages, actions, notices et sujets sont chiffrés et déchiffrés ; les messages chiffrés portent un badge *FiSH* et l'en-tête indique quand une conversation est chiffrée. Les clés sont rangées dans le trousseau du système.
- Logo de l'application : fenêtre, app macOS, exécutable Windows et paquet Linux (icône et entrée `.desktop`).
- Options reprises de mIRC : *Ne pas déranger* (☾ dans la barre latérale, `Mod+Maj+D`) coupe les notifications et alertes de fenêtre ; lignes ombrées en option ; un œil affiche les mots de passe du formulaire réseau ; clic du milieu pour fermer un onglet ; ⧉ copie tout un onglet dans le presse-papiers ; un message d'absence n'est affiché qu'une fois tant qu'il ne change pas (réglage pour le répéter).
- Transferts de fichiers DCC : `/dcc send <pseudo> <fichier>` ou glisser un fichier sur une conversation privée ; les propositions reçues attendent votre réponse dans le nouveau panneau *Transferts* (⇅), avec progression, vitesse, annulation et reprise des téléchargements interrompus. Mode passif pour qui est derrière une box, dossier de téléchargement et adresse annoncée dans les réglages *Discussion*.

## [0.2.0] - 2026-11-03

### Ajouté

- Identification par certificat client : SASL `EXTERNAL` et CertFP. Le formulaire du réseau peut créer un certificat auto-signé, rangé dans `certs/` à côté de la configuration.
- Interface en anglais avec traduction française ; nouveau réglage *Apparence → Langue* (langue du système par défaut).
- `draft/multiline` : un texte collé de plusieurs lignes part en un seul message quand le serveur le permet, et les messages de plusieurs lignes s'affichent avec leurs retours à la ligne.
- Proxy SOCKS5 par réseau, avec identifiant et mot de passe facultatifs ; le nom du serveur est résolu par le proxy (aucune fuite DNS avec Tor). Le mot de passe du proxy va dans le trousseau du système.

### Sécurité

- Mots de passe (serveur et SASL) rangés dans le trousseau du système : Trousseau d'accès sur macOS, Gestionnaire d'identifiants sur Windows, Secret Service sous Linux. Les mots de passe déjà présents dans le fichier y sont déplacés au prochain enregistrement ; réglage dans *Confidentialité*.

### Modifié

- Notes de version en anglais et en français, publiées avec chaque release.
- Barre latérale plus lisible : chaque réseau forme un bloc séparé par un filet, ses onglets sont regroupés sous « Salons » et « Messages privés » avec leur nombre, et un trait vertical les relie à leur réseau.
- Deux réseaux ne peuvent plus porter le même nom.
- Le code, les commentaires et la documentation sont en anglais ; le README existe aussi en français.

## [0.1.0] - 2026-10-29

### Ajouté

- Client IRC multi-réseaux à interface native (iced), inspiré de HexChat.
- Connexion TLS par défaut, SASL PLAIN, STS, reconnexion automatique, pseudonymes de secours.
- Plus de 25 extensions IRCv3 : historique (`chathistory`), messages lus, indicateur de saisie, suppression de messages, `MONITOR`, `WHOX`…
- 7 thèmes, couleur d'accent, taille du texte, colonne des pseudos réglable.
- Suggestions pendant la frappe : commandes, salons, pseudos, services (NickServ, ChanServ…) et commandes de bots.
- 37 raccourcis clavier reconfigurables, adaptés à macOS et à Windows.
- Recherche rapide d'onglet et recherche dans l'historique.
- Notifications du système, rebond du Dock, compteur dans le titre.
- Journaux sur disque, utilisateurs ignorés (`/ignore`), commandes exécutées après la connexion.
- Réglages de confidentialité : indicateur de saisie, accusés de lecture, réponses CTCP.

### Sécurité

- SASL refusé sans TLS, STS mémorisé uniquement sur certificat vérifié.
- Fichiers de configuration et journaux privés (`0600`), écriture atomique.
- Liens filtrés (`http`/`https`) et caractères bidirectionnels retirés de l'affichage.
- Limitation du débit d'envoi et des réponses CTCP, mémoire bornée face à un serveur hostile.
- Mots de passe NickServ masqués à l'affichage, dans les journaux et l'historique de saisie.

[Non publié]: https://github.com/BrystoQ/ircnext/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/BrystoQ/ircnext/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/BrystoQ/ircnext/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/BrystoQ/ircnext/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/BrystoQ/ircnext/releases/tag/v0.1.0
