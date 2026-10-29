# Politique de sécurité

## Versions prises en charge

Seule la dernière version publiée reçoit des correctifs de sécurité.

## Signaler une vulnérabilité

Merci de **ne pas** ouvrir de ticket public.

Utilisez le [signalement privé de GitHub](https://github.com/BrystoQ/ircnext/security/advisories/new) (onglet *Security* → *Report a vulnerability*). Indiquez :

- la nature de la faille et son impact ;
- les étapes ou le message IRC permettant de la reproduire ;
- la version et le système concernés.

Vous recevrez une réponse sous 7 jours. Une fois la faille corrigée, elle sera publiée avec vos remerciements, si vous le souhaitez.

## Périmètre

Sont notamment concernés :

- tout message d'un serveur ou d'un utilisateur IRC qui fait planter le client, consomme la mémoire sans limite ou injecte des commandes ;
- toute fuite d'identifiants (SASL, mot de passe serveur, NickServ) ;
- tout contournement des protections décrites dans le [README](README.md#sécurité) : TLS, STS, filtrage des liens, permissions des fichiers.
