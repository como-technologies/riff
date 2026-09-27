# Development

Install [Rust](https://rustup.rs) and [just](https://just.systems). Then:

```sh
just init   # once: installs the book and audit tools
just ci     # the gate: fmt, clippy, tests, API docs, book, requirement IDs
```

CI runs the same gate on each push. It publishes this book to GitHub
Pages.

The design docs are in the code. Read them in the
[API docs](api/riff_core/index.html).

## Build riff from your clone

Do [Start a Riff](start-a-riff.md), with one change: in step 1, build
from your clone. Then the binaries have your changes:

```sh
just install
```

`riff tail` shows the messages of the thread of your repository.

## Add a requirement

Each requirement in `docs/src/requirements.md` has an ID. A new
requirement gets a new ID from this command:

```sh
just rid
```

It prints a new ID of 26 digits and capital letters. Write the new
requirement as `- **ID** text`, with that ID. Cite it by the same ID
in code, tests and commits. Two people who add requirements at the same time never get
the same ID. So nobody has to agree on a number first.

The old IDs `R1` to `R232` stay. Never give a new requirement the next
`R` number, and never renumber a requirement.

`just ci` checks the IDs. To run only this check:

```sh
just reqs
```

It fails when two requirements have the same ID, or when a file cites an
ID that no requirement has. It warns when a new ID does not have that
form, for example the next `R` number.

## Check a pull request on GitHub

Each pull request has the same form. The form links the pull request to
its issue and to its wave. A squash merge copies it into the commit on
`main`. This is the body of a pull request for issue 77:

```text
Closes #77

Pause and resume the riff. A new riff starts paused.

Issue: #77
Milestone: Wave 3
```

- The first line links the issue. Use `Closes #N` in the last pull
  request of the issue. Use `Refs #N` in each other one, and when a
  check after the merge is left.
- The last lines are trailers: the issue, and its milestone.
- Give the pull request the milestone of its issue. Do not end the
  title with `(#N)`. GitHub adds the number of the pull request.

Write the body to a file, then open the pull request:

```sh
gh pr create --title "Pause and resume the riff" --milestone "Wave 3" --body-file pr.md
```

The job `Hygiene` checks each pull request. To check one yourself, give
its number:

```sh
just hygiene pr 90
```

To check the message of a commit:

```sh
just hygiene commit HEAD
```

Each error names its rule. The
[API docs of hygiene](api/hygiene/index.html) list the rules.

GitHub runs no check on a pull request that has a conflict with
`main`. When `Hygiene` does not run, rebase your branch on `main` and
push it again.

## Sign in on this machine

Sign-in uses the OAuth client of the Google Cloud project `como-riff`.
The client exists. To make it again, see
[Make the OAuth client](#make-the-oauth-client).

1. Do [Start a Riff](start-a-riff.md) first.

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
   `just local setup --admin EMAIL`.

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
`@`, in lower case. Each character that a URI part cannot hold becomes
`-`: `O'Brien@comotechnologies.io` gets the user `o-brien`. Your user
belongs to your email: no other account can sign in as it. `riff
logout` removes the sign-in from this device.

### When another account holds your user

Two emails can give the same user, for example `o'brien@` and
`o-brien@`. The first account that signs in holds the user. `riff
login` then stops for the second account:

```sh
riff login
```

```text
riff-server refused the token request: access_denied: the user o-brien
belongs to another account; ask an admin
```

Ask an admin which account holds the user. An admin can end the
sign-ins of that account (`riff logout --all --user USER`), but the
user still belongs to its email. To sign in, use an email that gives
another user.

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

### When riff says the riff has no sign-in

A riff can start again with no sign-in, for example after
[Start a Riff](start-a-riff.md). Then each command stops with
`riff-server at URL has no sign-in, but this machine has an old sign-in
for it`. Remove the old sign-in:

```sh
riff logout
```

Then start your Claude Code sessions again. Your user is now the name
that you log in with on your machine.

### When riff says a session is known as another user

The server keeps one user for each session. A call with the same
session ID and another user fails with `session ID is known as user
USER`. That happens when the session started before `riff login`. Start
the Claude Code session again, as a new session.
`riff logout --all` ends each of your sign-ins, on each device.

Only accounts of `comotechnologies.io` can sign in. To allow another
Workspace domain, add `--allowed-domain DOMAIN` to `just local setup`.
Give `--allowed-domain` once for each domain, the default domain too.
Two domains do not share a user: `alice@a.com` and `alice@b.com` both
give `alice`, and only the first account gets it.

An admin can end each sign-in of another person. Name each admin by
verified email, once for each admin:

```sh
just local setup --admin alice@comotechnologies.io
```

A name that is not an email names nobody: the log says so at start.
Then an admin runs:

```sh
riff logout --all --user USER
```

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

- Each install keeps the old settings that it does not get again. A
  setting that it gets replaces the old one. So a plain
  `riff-server install` keeps the OAuth client, `--listen` and
  `--insecure`.
- With no sign-in, an address that is not loopback needs `--insecure`.
  Else `install` fails and changes nothing.
- `riff-server uninstall` stops the service and removes its files.

### Remove a setting of the service

An install cannot remove an old setting. Uninstall, then install with
the settings that you want:

```sh
riff-server uninstall
riff-server install --listen 127.0.0.1:7878
```

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
Then each push to `main` that changes the code of a crate builds the
image and deploys it, after the gate passes. A `riff` refuses a
server of another build (see
[Builds](how-it-works.md#builds)), so the server follows each code
change. For now, the variable is not set,
and no shared server runs. The job signs in to
Google Cloud from GitHub with no key. See the deploys:

```sh
gh run list --workflow CI --branch main
```

`just cloud` alone lists its recipes.

### Turn the shared server on

It sets `CLOUD_DEPLOY`, and deploys now. While the service runs, it
costs about 45 USD each month: one instance with 1 vCPU that is always
on (R29). Turn it off when nobody uses it. For the cost of each host,
see [#47](https://github.com/como-technologies/riff/issues/47).

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
