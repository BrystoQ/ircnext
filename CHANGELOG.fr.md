# Notes de version

[English](CHANGELOG.md) · **Français**

Ce projet suit le [versionnage sémantique](https://semver.org/lang/fr/). Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/).

## [Non publié]

### Ajouté

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

[Non publié]: https://github.com/BrystoQ/ircnext/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/BrystoQ/ircnext/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/BrystoQ/ircnext/releases/tag/v0.1.0
