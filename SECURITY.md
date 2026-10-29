# Security policy

## Supported versions

Only the latest released version receives security fixes.

## Reporting a vulnerability

Please do **not** open a public issue.

Use [GitHub private reporting](https://github.com/BrystoQ/ircnext/security/advisories/new) (*Security* tab → *Report a vulnerability*). Include:

- the nature of the vulnerability and its impact;
- the steps or the IRC message that reproduce it;
- the version and the system involved.

You will get an answer within 7 days. Once fixed, the vulnerability will be published with credit to you, if you wish.

## Scope

This includes in particular:

- any message from an IRC server or user that crashes the client, uses unbounded memory or injects commands;
- any credential leak (SASL, server password, client certificate key, NickServ);
- any bypass of the protections described in the [README](README.md#security): TLS, STS, keychain, link filtering, file permissions.
