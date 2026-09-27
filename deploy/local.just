# riff-server on this machine, as a systemd user service (R119).
# Recipes run in deploy/.

# List the recipes of `just local`
default:
    @just --list local

# Install the service with the OAuth client of the cloud project. It replaces all settings
setup *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    . ./cloud.env
    if [ -z "$RIFF_OIDC_CLIENT_ID" ]; then
        echo "deploy/cloud.env has no client ID. Run: just cloud oauth-client" >&2
        exit 1
    fi
    secret=$(gcloud secrets versions access latest --secret "$CLOUD_SECRET" --project "$CLOUD_PROJECT")
    RIFF_OIDC_CLIENT_ID=$RIFF_OIDC_CLIENT_ID RIFF_OIDC_CLIENT_SECRET=$secret riff-server install {{ARGS}}

# Start the service
start:
    systemctl --user start riff-server

# Stop the service. Sessions lose the connection until it starts again
stop:
    systemctl --user stop riff-server

# Restart the service
restart:
    systemctl --user restart riff-server

# Show the state of the service, and check that it answers
status:
    #!/usr/bin/env bash
    set -uo pipefail
    echo "service: $(systemctl --user is-active riff-server)"
    listen=$(sed -n 's/^RIFF_LISTEN="\{0,1\}\([^"]*\)"\{0,1\}$/\1/p' ~/.config/systemd/user/riff-server.env 2>/dev/null)
    url="http://${listen:-127.0.0.1:7878}"
    code=$(curl -s -m 3 -o /dev/null -w '%{http_code}' "$url/v1/sign-in")
    if [ "$code" = 000 ]; then echo "url: $url (no answer)"; else echo "url: $url (answers, HTTP $code)"; fi
    echo "binary: $(riff-server --version 2>/dev/null || echo 'not installed')"

# Show the log. Add -f to follow it
log *ARGS='-n 50':
    journalctl --user -u riff-server --no-pager {{ARGS}}
