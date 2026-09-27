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

gcloud services enable secretmanager.googleapis.com storage.googleapis.com \
    iam.googleapis.com run.googleapis.com cloudbuild.googleapis.com \
    artifactregistry.googleapis.com iamcredentials.googleapis.com \
    sts.googleapis.com "${project[@]}"
echo "APIs: on."

# IAM needs some seconds before it knows a new service account. So try
# each role again for up to one minute.
bind() {
    for _ in 1 2 3 4 5 6; do
        if "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 10
    done
    "$@" >/dev/null
}

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

account() { echo "$1@$CLOUD_PROJECT.iam.gserviceaccount.com"; }
for name in "$CLOUD_RUN_ACCOUNT" "$CLOUD_BUILD_ACCOUNT" "$CLOUD_DEPLOY_ACCOUNT"; do
    if gcloud iam service-accounts describe "$(account "$name")" "${project[@]}" >/dev/null 2>&1; then
        echo "Service account $name: exists."
    else
        echo "Service account $name: making it."
        gcloud iam service-accounts create "$name" --display-name "$name" "${project[@]}"
    fi
done
# riff-server reads and writes only its bucket, and reads only its
# secret (R134). The build account may only build and store images.
run_account=serviceAccount:$(account "$CLOUD_RUN_ACCOUNT")
bind gcloud storage buckets add-iam-policy-binding "$bucket" --member "$run_account" \
    --role roles/storage.objectUser "${project[@]}"
bind gcloud secrets add-iam-policy-binding "$CLOUD_SECRET" --member "$run_account" \
    --role roles/secretmanager.secretAccessor "${project[@]}"
bind gcloud projects add-iam-policy-binding "$CLOUD_PROJECT" \
    --member "serviceAccount:$(account "$CLOUD_BUILD_ACCOUNT")" \
    --role roles/run.builder --condition None
echo "Service accounts: roles set."

# CI builds the image, pushes it to this repository, and deploys it
# (R160).
where=(--location "$CLOUD_REGION" "${project[@]}")
if gcloud artifacts repositories describe "$CLOUD_REPOSITORY" "${where[@]}" >/dev/null 2>&1; then
    echo "Image repository $CLOUD_REPOSITORY: exists."
else
    echo "Image repository $CLOUD_REPOSITORY: making it."
    gcloud artifacts repositories create "$CLOUD_REPOSITORY" --repository-format docker "${where[@]}"
fi

# GitHub Actions signs in with its OIDC token, only from the main branch
# of the repository (R161). No key exists.
pool=(--workload-identity-pool github --location global "${project[@]}")
if gcloud iam workload-identity-pools describe github --location global "${project[@]}" >/dev/null 2>&1; then
    echo "Identity pool github: exists."
else
    echo "Identity pool github: making it."
    gcloud iam workload-identity-pools create github --location global \
        --display-name "GitHub Actions" "${project[@]}"
fi
if gcloud iam workload-identity-pools providers describe github "${pool[@]}" >/dev/null 2>&1; then
    echo "Identity provider github: exists."
else
    echo "Identity provider github: making it."
    gcloud iam workload-identity-pools providers create-oidc github "${pool[@]}" \
        --issuer-uri https://token.actions.githubusercontent.com \
        --attribute-mapping google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref \
        --attribute-condition "assertion.repository == '$CLOUD_GITHUB_REPO' && assertion.ref == 'refs/heads/main'"
fi

# The deploy account pushes images, deploys the service, and runs it as
# riff-server. Only the repository may use the account (R161).
deploy_account=serviceAccount:$(account "$CLOUD_DEPLOY_ACCOUNT")
bind gcloud artifacts repositories add-iam-policy-binding "$CLOUD_REPOSITORY" \
    --member "$deploy_account" --role roles/artifactregistry.writer "${where[@]}"
bind gcloud projects add-iam-policy-binding "$CLOUD_PROJECT" --member "$deploy_account" \
    --role roles/run.admin --condition None
bind gcloud iam service-accounts add-iam-policy-binding "$(account "$CLOUD_RUN_ACCOUNT")" \
    --member "$deploy_account" --role roles/iam.serviceAccountUser "${project[@]}"
github=principalSet://iam.googleapis.com/projects/$CLOUD_PROJECT_NUMBER/locations/global/workloadIdentityPools/github/attribute.repository/$CLOUD_GITHUB_REPO
bind gcloud iam service-accounts add-iam-policy-binding "$(account "$CLOUD_DEPLOY_ACCOUNT")" \
    --member "$github" --role roles/iam.workloadIdentityUser "${project[@]}"
echo "CI deploy: set."

# gcloud warns when it filters an empty list, so hide its stderr.
versions=$(gcloud secrets versions list "$CLOUD_SECRET" --filter=state=ENABLED \
    --limit 1 --format="value(name)" "${project[@]}" 2>/dev/null)
if [ -z "$RIFF_OIDC_CLIENT_ID" ] || [ -z "$versions" ]; then
    echo
    echo "Next: make the OAuth client by hand. See \"Make the OAuth client\""
    echo "on the Development page of the book. Then run: just cloud oauth-client"
fi
