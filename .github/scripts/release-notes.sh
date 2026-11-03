#!/bin/sh
# Prints the release notes of version $1 (without the leading "v"): its
# section of CHANGELOG.md, then the French one of CHANGELOG.fr.md. Fails if
# either is missing, so that no version goes out without patch notes.
set -eu
version="$1"

section() {
  awk -v version="$version" '
    index($0, "## [" version "]") == 1 { found = 1; next }
    found && /^## \[/ { exit }
    found && /^\[[^]]*\]: / { exit }
    found { print }
  ' "$1" | sed -e '/./,$!d'
}

english=$(section CHANGELOG.md)
french=$(section CHANGELOG.fr.md)
if [ -z "$english" ] || [ -z "$french" ]; then
  echo "::error::No patch notes for $version in CHANGELOG.md and CHANGELOG.fr.md." >&2
  exit 1
fi

printf '%s\n\n' "$english"
printf '<details>\n<summary>Notes de version en français</summary>\n\n%s\n\n</details>\n' "$french"
