# The shared server on Cloud Run (R5, R181, 01M3NJAZ6BYH7TWKDYTVEK78PG).
# Recipes run in deploy/.
#
# NAME names the settings: `stage` reads deploy/stage.env. With no name,
# a recipe uses deploy/cloud.env, the settings of the shared riff
# (01M3ZE3Z580RB5AYAJX6321DFW). deploy/settings.sh reads them.

# List the recipes of `just cloud`
default:
    @just --list cloud

# Make the cloud resources of a riff; it checks each one first (R136)
setup NAME='':
    ./cloud-setup.sh {{NAME}}

# Store the OAuth client of a riff: the secret in Secret Manager, the ID in its settings
oauth-client NAME='':
    ./oauth-client.sh {{NAME}}

# Deploy now: the image of a release tag, or an image that Cloud Build builds from this tree (R136)
deploy NAME='' TAG='':
    ./deploy.sh {{NAME}} {{TAG}}

# Turn the shared server on: CI can deploy at the end of each wave, and this deploys now
up:
    gh variable set CLOUD_DEPLOY --body true
    ./deploy.sh

# Turn a riff off: the service is deleted, and the state stays in the bucket. For the shared riff, CI stops its deploys
down NAME='':
    #!/usr/bin/env bash
    set -euo pipefail
    set -- {{NAME}}
    . ./settings.sh
    if [ -z "$CLOUD_NAME" ] && [ -n "$(gh variable get CLOUD_DEPLOY 2>/dev/null)" ]; then
        gh variable delete CLOUD_DEPLOY
    fi
    where=(--project "$CLOUD_PROJECT" --region "$CLOUD_REGION")
    if gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" >/dev/null 2>&1; then
        gcloud run services delete "$CLOUD_SERVICE" "${where[@]}"
    else
        echo "No service runs."
    fi

# Show the state of the service of a riff, and for the shared riff if CI deploys
status NAME='':
    #!/usr/bin/env bash
    set -euo pipefail
    set -- {{NAME}}
    . ./settings.sh
    if [ -z "$CLOUD_NAME" ]; then
        echo "CI can deploy: $(gh variable get CLOUD_DEPLOY 2>/dev/null || echo 'no (CLOUD_DEPLOY is not set)')"
    fi
    where=(--project "$CLOUD_PROJECT" --region "$CLOUD_REGION")
    if gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" >/dev/null 2>&1; then
        gcloud run services describe "$CLOUD_SERVICE" "${where[@]}" \
            --format='value[separator="\n"](format("service: ready={0}", status.conditions[0].status), format("url: {0}", status.url), format("revision: {0}", status.latestReadyRevisionName))'
    else
        echo "service: none"
    fi

# Show the log of the service of a riff: [NAME] [gcloud options]. With no --limit, the last 50 lines
[positional-arguments]
log *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    . ./settings.sh
    case " $* " in
        *" --limit "*|*" --limit="*) ;;
        *) set -- --limit 50 "$@" ;;
    esac
    gcloud run services logs read "$CLOUD_SERVICE" --project "$CLOUD_PROJECT" --region "$CLOUD_REGION" "$@"

# Show the log lines of the service with the severity ERROR or more: [NAME] [LIMIT]
[positional-arguments]
errors *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    . ./settings.sh
    gcloud run services logs read "$CLOUD_SERVICE" --project "$CLOUD_PROJECT" --region "$CLOUD_REGION" \
        --limit "${1:-50}" --log-filter 'severity>=ERROR'
