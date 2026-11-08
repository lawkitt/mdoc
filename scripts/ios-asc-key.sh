#!/usr/bin/env bash
# Usage: scripts/ios-asc-key.sh <folder>
# Writes the App Store Connect API key from the environment (ADR 0107) to
# <folder>/AuthKey_<ASC_KEY_ID>.p8, readable by its owner only, and prints
# that path. ASC_KEY_P8 is the key's base64 body without its BEGIN and END
# lines, as `release-ios` holds it; a full PEM works too. ios-release.yml
# runs it into the runner's temp folder and deletes the folder when the job
# ends. Prints nothing of the key. Bash 3.2.
source "$(dirname "$0")/lib.sh"
dir=${1:?usage: scripts/ios-asc-key.sh <folder>}
: "${ASC_KEY_ID:?ASC_KEY_ID is not set}"
: "${ASC_KEY_P8:?ASC_KEY_P8 is not set}"
[[ $ASC_KEY_ID =~ ^[A-Za-z0-9]+$ ]] || die "ASC_KEY_ID is not a key ID"

mkdir -p "$dir"
chmod 700 "$dir"
key=$dir/AuthKey_$ASC_KEY_ID.p8
umask 077
# The body alone: drop any BEGIN/END lines, then every space and newline
# (a secret pasted with literal \n too), and fold it as PEM does.
body=$(printf '%s\n' "$ASC_KEY_P8" | sed -e '/-----BEGIN/d' -e '/-----END/d' -e 's/\\n//g' | tr -d ' \t\r\n')
[[ $body =~ ^[A-Za-z0-9+/]+=*$ ]] || die "ASC_KEY_P8 is not a base64 key body"
{
  echo "-----BEGIN PRIVATE KEY-----"
  printf '%s\n' "$body" | fold -w 64
  echo "-----END PRIVATE KEY-----"
} >"$key"
openssl pkey -in "$key" -noout 2>/dev/null || die "ASC_KEY_P8 does not hold a private key"
echo "$key"
