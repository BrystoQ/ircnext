# Journal des modifications

Ce projet suit le [versionnage sémantique](https://semver.org/lang/fr/). Le format s'inspire de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/).

## [Non publié]

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

[Non publié]: https://github.com/BrystoQ/ircnext/commits/main
