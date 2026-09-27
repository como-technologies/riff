#!/usr/bin/env bash
# Deploys riff-server to Cloud Run (R5, R6, R29, R32, R130, R131,
# R134). Run `just cloud setup` first.
#
# With no argument, Cloud Build builds the image from the source, and
# the script maps the domain once. With `--image IMAGE`, the script
# deploys that image and does nothing else. CI does that (R160).
set -euo pipefail
cd "$(dirname "$0")/.."
. deploy/cloud.env
where=(--project "$CLOUD_PROJECT" --region "$CLOUD_REGION")
account() { echo "$1@$CLOUD_PROJECT.iam.gserviceaccount.com"; }

if [ "${1:-}" = --image ] && [ -n "${2:-}" ]; then
    image=$2
    from=(--image "$image")
else
    image=
    from=(--source . --build-service-account
        "projects/$CLOUD_PROJECT/serviceAccounts/$(account "$CLOUD_BUILD_ACCOUNT")")
fi

if [ -z "$RIFF_OIDC_CLIENT_ID" ]; then
    echo "deploy/cloud.env has no client ID. Run: just cloud oauth-client" >&2
    exit 1
fi

# One instance, with its CPU on also between calls. 1000 calls at a
# time, each for up to 60 minutes. riff checks each token itself, so
# Cloud Run lets each caller in.
gcloud run deploy "$CLOUD_SERVICE" "${from[@]}" --quiet "${where[@]}" \
    --service-account "$(account "$CLOUD_RUN_ACCOUNT")" \
    --port 8080 --min-instances 1 --max-instances 1 --no-cpu-throttling \
    --concurrency 1000 --timeout 3600 --no-invoker-iam-check \
    --set-env-vars "RIFF_PUBLIC_URL=$CLOUD_URL,RIFF_REQUIRE_SIGN_IN=true,RIFF_OIDC_CLIENT_ID=$RIFF_OIDC_CLIENT_ID,RIFF_BUCKET=$CLOUD_BUCKET" \
    --set-secrets "RIFF_OIDC_CLIENT_SECRET=$CLOUD_SECRET:latest"

if [ -n "$image" ] || [ "$CLOUD_URL" != "https://$CLOUD_DOMAIN" ]; then
    exit 0
fi
# Only the beta commands take --region.
if gcloud beta run domain-mappings describe --domain "$CLOUD_DOMAIN" "${where[@]}" >/dev/null 2>&1; then
    echo "Domain $CLOUD_DOMAIN: mapped."
else
    # gcloud shows the DNS records to add.
    echo "Domain $CLOUD_DOMAIN: mapping it."
    gcloud beta run domain-mappings create --service "$CLOUD_SERVICE" --domain "$CLOUD_DOMAIN" "${where[@]}"
fi
