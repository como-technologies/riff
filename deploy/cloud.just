# The shared server on Cloud Run (R5, R181, 01M3NJAZ6BYH7TWKDYTVEK78PG).
# Recipes run in deploy/.

# List the recipes of `just cloud`
default:
    @just --list cloud

# Make the cloud resources of riff; it checks each one first (R136)
setup:
    ./cloud-setup.sh

# Store the OAuth client: the secret in Secret Manager, the ID in cloud.env
oauth-client:
    ./oauth-client.sh

# Build the image with Cloud Build and deploy it now (R136)
deploy:
    ./deploy.sh

# Turn the shared server on: CI can deploy at the end of each wave, and this deploys now
up:
    gh variable set CLOUD_DEPLOY --body true
    ./deploy.sh

# Turn the shared server off: CI stops its deploys, and the service is deleted. The state stays in the bucket
down:
    #!/usr/bin/env bash
    set -euo pipefail
    . ./cloud.env
    if [ -n "$(gh variable get CLOUD_DEPLOY 2>/dev/null)" ]; then
        gh variable delete CLOUD_DEPLOY
    fi
    where=(--project "$CLOUD_PROJECT" --region "$CLOUD_REGION")
    if gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" >/dev/null 2>&1; then
        gcloud run services delete "$CLOUD_SERVICE" "${where[@]}"
    else
        echo "No service runs."
    fi

# Show if CI deploys, and the state of the service
status:
    #!/usr/bin/env bash
    set -euo pipefail
    . ./cloud.env
    echo "CI can deploy: $(gh variable get CLOUD_DEPLOY 2>/dev/null || echo 'no (CLOUD_DEPLOY is not set)')"
    where=(--project "$CLOUD_PROJECT" --region "$CLOUD_REGION")
    if gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" >/dev/null 2>&1; then
        gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" \
            --format='value[separator="\n"](format("service: ready={0}", status.conditions[0].status), format("url: {0}", status.url), format("revision: {0}", status.latestReadyRevisionName))'
    else
        echo "service: none"
    fi

# Show the log of the service
log *ARGS='--limit 50':
    #!/usr/bin/env bash
    set -euo pipefail
    . ./cloud.env
    gcloud run services logs read "$CLOUD_SERVICE" --project "$CLOUD_PROJECT" --region "$CLOUD_REGION" {{ARGS}}
