#!/usr/bin/env bash
# Makes the resources of riff in its cloud project (R136, R143). It
# checks each resource first, so it can run again. A person makes the
# project and links its billing account with gcloud, by the how-to in
# the book.
set -euo pipefail
cd "$(dirname "$0")"
. ./cloud.env
project=(--project "$CLOUD_PROJECT")
howto='See "Set up the cloud project" on the Development page of the book.'

if ! gcloud projects describe "$CLOUD_PROJECT" >/dev/null 2>&1; then
    echo "You cannot see the project $CLOUD_PROJECT. $howto" >&2
    exit 1
fi
if [ "$(gcloud billing projects describe "$CLOUD_PROJECT" --format='value(billingEnabled)')" != True ]; then
    echo "The project $CLOUD_PROJECT has no billing account. $howto" >&2
    exit 1
fi
echo "Project $CLOUD_PROJECT: exists, with billing."

gcloud services enable secretmanager.googleapis.com storage.googleapis.com "${project[@]}"
echo "APIs: on."

if gcloud secrets describe "$CLOUD_SECRET" "${project[@]}" >/dev/null 2>&1; then
    echo "Secret $CLOUD_SECRET: exists."
else
    echo "Secret $CLOUD_SECRET: making it."
    gcloud secrets create "$CLOUD_SECRET" --replication-policy automatic "${project[@]}"
fi

bucket=gs://$CLOUD_BUCKET
if gcloud storage buckets describe "$bucket" "${project[@]}" >/dev/null 2>&1; then
    echo "Bucket $CLOUD_BUCKET: exists."
else
    echo "Bucket $CLOUD_BUCKET: making it."
    gcloud storage buckets create "$bucket" --location "$CLOUD_REGION" \
        --uniform-bucket-level-access --public-access-prevention "${project[@]}"
fi
# The rule deletes each thread object 30 days after its last change (R46).
gcloud storage buckets update "$bucket" --lifecycle-file lifecycle.json "${project[@]}"
echo "Bucket $CLOUD_BUCKET: lifecycle rule set."

# gcloud warns when it filters an empty list, so hide its stderr.
versions=$(gcloud secrets versions list "$CLOUD_SECRET" --filter=state=ENABLED \
    --limit 1 --format="value(name)" "${project[@]}" 2>/dev/null)
if [ -z "$RIFF_OIDC_CLIENT_ID" ] || [ -z "$versions" ]; then
    echo
    echo "Next: make the OAuth client by hand. See \"Make the OAuth client\""
    echo "on the Development page of the book. Then run: just oauth-client"
fi
