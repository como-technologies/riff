#!/usr/bin/env bash
# Stores the OAuth client of riff (R145): the secret in Secret Manager,
# the ID in the settings file. The secret goes to no file.
#
# Usage: oauth-client.sh [NAME]. NAME names the settings, for example
# `stage` (01M3ZE3Z580RB5AYAJX6321DFW). With no name, the shared riff.
set -euo pipefail
cd "$(dirname "$0")"
. ./settings.sh
file=$(basename "$CLOUD_SETTINGS")

read -rp "Client ID: " id
if ! [[ $id =~ ^[0-9a-z-]+\.apps\.googleusercontent\.com$ ]]; then
    echo "A Google client ID ends in .apps.googleusercontent.com." >&2
    exit 1
fi
read -rsp "Client secret (hidden): " secret
echo
if [ -z "$secret" ]; then
    echo "The client secret is empty." >&2
    exit 1
fi

printf %s "$secret" | gcloud secrets versions add "$CLOUD_SECRET" \
    --project "$CLOUD_PROJECT" --data-file=- >/dev/null
unset secret
echo "Secret $CLOUD_SECRET: stored."

sed -i.bak "s/^RIFF_OIDC_CLIENT_ID=.*/RIFF_OIDC_CLIENT_ID=$id/" "$file"
rm "$file.bak"
echo "Client ID: written to deploy/$file. Commit that file."
