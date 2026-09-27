# Development

Install [Rust](https://rustup.rs) and [just](https://just.systems). Then:

```sh
just init   # once: installs the book and audit tools
just ci     # the gate: fmt, clippy, tests, API docs, book
```

CI runs the same gate on each push. It publishes this book to GitHub
Pages.

The design docs are in the code. Read them in the
[API docs](api/riff_core/index.html).

## Try it on one machine

Do these steps in order. They need no sign-in and no cloud.

1. Install `riff` and `riff-server`:

   ```sh
   just install
   ```

2. Start the server as a service. It starts at login and restarts
   after a crash:

   ```sh
   riff-server install
   ```

   On a machine without systemd, run `riff-server` in a terminal
   instead, and keep the terminal open.

3. Install the Claude Code plugin:

   ```sh
   riff connect claude
   ```

4. Start two Claude Code sessions. They can share a directory: each
   session has its own session ID. The start hook tells each session to
   run `riff watch --once` as a background task.

5. In one session, say: *"Post to the other session with riff."* The
   agent finds the other session with `who` and puts its session ID in
   `to`.

The post output names the session that woke. The other session wakes
and reads the message. `riff tail` shows the thread.

## Sign in on this machine

Sign-in uses the OAuth client of the Google Cloud project `como-riff`.
The client exists. To make it again, see
[Make the OAuth client](#make-the-oauth-client).

1. Do [Try it on one machine](#try-it-on-one-machine) first.

2. Install the [gcloud CLI](https://cloud.google.com/sdk/docs/install).
   Sign in with a Como account that can read the client secret in
   `como-riff`:

   ```sh
   gcloud auth login
   ```

3. Install the server service again, with the OAuth client:

   ```sh
   just local setup
   ```

   `just local setup` runs `riff-server install` with the client ID from
   `deploy/cloud.env` and the client secret from Secret Manager. It
   gives its own options to `riff-server install`, for example
   `just local setup --admin USER`.

4. Check the server log:

   ```sh
   just local log -n 5
   ```

   The last start must show `sign-in with https://accounts.google.com`
   and `the provider knows the OAuth client`. If it shows `nobody can
   sign in`, the service has no client: do step 3 again. If it shows
   `riff-server stops`, Google refused the client. The line says why.

5. Sign in. Your browser opens. Pick your Como account:

   ```sh
   riff login
   ```

6. Start your Claude Code sessions again. A session that started
   before `riff login` has no token.

The user part of your URI is now the part of your email before the
`@`. `riff logout` removes the sign-in from this device.

### When riff cannot read the keyring

`riff` keeps your sign-in in the OS keyring. When it cannot read the
keyring, each command stops with `riff cannot find your user`. The
next lines say why. Unlock the keyring and run the command again. On
Linux, a Secret Service must run, for example GNOME Keyring.

On a machine with no keyring, name your user yourself:

```sh
export RIFF_USER=USER
```

`riff` then sends no token. A server with `--require-sign-in` refuses
it.

### When riff says a session is known as another user

The server keeps one user for each session. A call with the same
session ID and another user fails with `session ID is known as user
USER`. That happens when the session started before `riff login`. Start
the Claude Code session again, as a new session.
`riff logout --all` ends each of your sign-ins, on each device.

Only accounts of `comotechnologies.io` can sign in. To allow another
Workspace domain, add `--allowed-domain DOMAIN` to `just local setup`.
Give `--allowed-domain` once for each domain, the default domain too.

An admin can end each sign-in of another person. Name the admins with
`--admin USER`, once for each admin. Then an admin runs
`riff logout --all --user USER`.

With `--require-sign-in`, the server refuses each call without a
token. `riff` sends a token on each call when you are signed in. A
command that you type acts as you. Each Claude Code session gets its
own token, which acts only as that session.

Each token works only with the device key of this machine. `riff`
keeps the key in the OS keyring. Use the same server URL for `riff`
(`RIFF_SERVER`) as the server has for itself (`--public-url`, by
default `http://` and the listen address). Else the server refuses
each proof.

## The server service

`riff-server install` writes its settings, as options or as `RIFF_*`
variables, to `~/.config/systemd/user/riff-server.env`, with mode
0600. Then it enables and starts the service. `just local setup` does
the same, with the OAuth client.

- Each install replaces all settings. When you use sign-in, always use
  `just local setup`: plain `riff-server install` removes the OAuth
  client.
- `riff-server uninstall` stops the service and removes its files.

The service stops when you log out. To keep it running, run
`loginctl enable-linger` once.

`just local` alone lists its recipes.

### Check the local server

It shows the state of the service, if the server answers, and the
version of the binary:

```sh
just local status
```

### Start and stop the local server

```sh
just local stop
just local start
just local restart
```

### See the local log

Add `-f` to follow the log:

```sh
just local log
just local log -f
```

### Update the local server

Build the new binaries, install the service again, and update the
plugin:

```sh
just install
just local setup
riff connect claude
```

### Point riff at a server

`riff` uses the local server when `RIFF_SERVER` is not set. `just use`
prints the shell line for a server. Run it with `eval` in each shell
where you run `riff` or start Claude Code:

```sh
eval "$(just use cloud)"
eval "$(just use local)"
just use
```

`just use` alone shows the server that `riff` uses now.

## Save the state in a bucket

Without a bucket, `riff-server` keeps its state only in memory. A
restart loses all threads, sessions and claims. With `--bucket`
(`RIFF_BUCKET`), the server loads the state from a Cloud Storage bucket
at start. It then saves each change to the bucket:

```sh
riff-server --bucket como-riff-state
```

The server gets its access token from the metadata server of Cloud
Run. So the flag works only on Cloud Run. The service account of the
server must have write access to the bucket.

With a bucket, the server takes the lease at start, waits 15 seconds,
and then loads the state. Only one server serves from a bucket. When a
new server takes the lease, the old one replies 503 and exits after 60
seconds.

## Set up the cloud project

Do this once, for the team. The project `como-riff` exists: use these
steps only to make it again.

The Google Cloud project `como-riff` holds each cloud resource of
riff. `deploy/cloud.env` holds its settings. The repository is public.
Put no email address, billing account ID or organization ID in it.

Install the [gcloud CLI](https://cloud.google.com/sdk/docs/install).
Sign in with your Como account:

```sh
gcloud auth login
```

Make the project once. The first command shows the ID of the
organization:

```sh
gcloud organizations list
gcloud projects create como-riff --organization ORGANIZATION_ID
```

Link a billing account to the project. The first command shows the
accounts that you can use:

```sh
gcloud billing accounts list
gcloud billing projects link como-riff --billing-account BILLING_ACCOUNT_ID
```

Then make the resources of riff in the project:

```sh
just cloud setup
```

The command checks each resource first, so you can run it again.
It makes the bucket `como-riff-state` and two service accounts.
`riff-server` runs as `riff-server`. That account can read and write
only the bucket, and read only the client secret. Cloud Build builds
the image as `riff-build`. The bucket has one lifecycle
rule. The rule deletes each thread object 30 days after its last
change. The rule does not touch the sessions, the tokens or the lease.

### See the lifecycle rule

```sh
gcloud storage buckets describe gs://como-riff-state --project como-riff \
  --format 'json(lifecycle_config)'
```

The output shows one `Delete` rule with `age` 30 and the prefix
`threads/`.

## Make the OAuth client

Do this once, for the team. The client exists: use these steps only to
make it again.

riff signs in with one Google OAuth client. Google has no API to make
it, so you make it by hand, in the console. The console can use
slightly different words.

1. Open [Google Auth Platform](https://console.cloud.google.com/auth/overview?project=como-riff)
   and click **Get started**.
2. App information: the app name is `riff`. The support email is a
   group of the team, or your own address.
3. Audience: **Internal**. Then only accounts of the organization can
   sign in.
4. Contact information: the same address. Agree to the policy and
   click **Create**.
5. Data access: add nothing. riff asks only for `openid` and `email`.
6. Click **Clients**, then **Create client**. The application type is
   **Desktop app**. The name is `riff`. Click **Create**.
7. Keep the dialog open. It shows the secret only once. Do not
   download the JSON file. In a terminal, run:

   ```sh
   just cloud oauth-client
   ```

   It asks for the client ID and the client secret. It puts the secret
   in Secret Manager and the ID in `deploy/cloud.env`.
8. Commit `deploy/cloud.env`.

To see who changed the client, and when:

```sh
gcloud logging read 'protoPayload.serviceName="clientauthconfig.googleapis.com"' \
  --project como-riff --freshness 400d \
  --format 'table(timestamp, protoPayload.methodName, protoPayload.authenticationInfo.principalEmail)'
```

Google deletes a client that nobody uses for six months. It sends an
email 30 days before.

## Deploy

Do [Set up the cloud project](#set-up-the-cloud-project) and
[Make the OAuth client](#make-the-oauth-client) first.

CI deploys riff when the repository variable `CLOUD_DEPLOY` is `true`.
Then each push to `main` that changes the server builds the image and
deploys it, after the gate passes. For now, the variable is not set,
and no shared server runs. The job signs in to
Google Cloud from GitHub with no key. See the deploys:

```sh
gh run list --workflow CI --branch main
```

`just cloud` alone lists its recipes.

### Turn the shared server on

It sets `CLOUD_DEPLOY`, and deploys now:

```sh
just cloud up
```

### Turn the shared server off

It removes `CLOUD_DEPLOY`, and deletes the service. gcloud asks you
first. The state stays in the bucket. `just cloud up` makes the service
again, at the same URL.

```sh
just cloud down
```

### Deploy by hand

Cloud Build builds the image from `Dockerfile`. Cloud Run then runs one
instance of the service `riff-server`, with sign-in. It does not change
`CLOUD_DEPLOY`:

```sh
just cloud deploy
```

### Map the domain

For now, riff uses the Cloud Run URL of the service. Later, Cloud Run
serves riff at `riff.comotechnologies.io`. Do this once:

1. Google must know that you own the domain. In
   [Search Console](https://search.google.com/search-console), add a
   **Domain** property `comotechnologies.io`, and verify it.
2. At the DNS host of `comotechnologies.io`, add a CNAME record: name
   `riff`, value `ghs.googlehosted.com`.
3. In `deploy/cloud.env`, set `CLOUD_URL=https://riff.comotechnologies.io`.
4. Run `just cloud deploy`. It maps the domain to the service once. Google
   then makes the certificate. That can take some hours.

### Check the shared server

The first command shows if CI deploys, and the state of the service.
The next ones point `riff` at the shared server and sign in:

```sh
just cloud status
eval "$(just use cloud)"
riff login
riff who
```

### See the shared log

```sh
just cloud log
just cloud log --limit 20
```

Each start shows `the provider knows the OAuth client`. When Google
refuses the client, the log shows `riff-server stops` and why.
