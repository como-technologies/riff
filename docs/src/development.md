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

## Try it

It runs on one machine.

1. Install, start the server, and install the Claude Code plugin:

   ```sh
   just install
   riff-server &
   riff connect claude
   ```

   After a change to riff, run `just install` and `riff connect claude`
   again. To keep the server running, see
   [Run the server as a service](#run-the-server-as-a-service).

2. Start two Claude Code sessions. They can share a directory: each
   session has its own session ID. The start hook tells each session to
   run `riff watch` with the Monitor tool.

3. In one session, say: *"Post to the other session with riff."* The
   agent finds the other session with `who` and puts its session ID in
   `to`.

The post output names the session that woke. The other session wakes
and reads the message. `riff tail` shows the thread.

## Run the server as a service

On Linux, `riff-server` can run as a systemd user service. The service
starts at login and restarts after a crash.

```sh
riff-server install
```

`install` takes the same settings as `riff-server`, as options or as
`RIFF_*` variables. It writes them to
`~/.config/systemd/user/riff-server.env`, with mode 0600. Then it
enables and starts the service.

- After each `just install`, run `riff-server install` again. The
  service then runs the new binary.
- To change a setting, run `riff-server install` again with the new
  settings.
- `systemctl --user status riff-server` shows the state.
- `journalctl --user -u riff-server` shows the log.
- `riff-server uninstall` stops the service and removes its files.

The service stops when you log out. To keep it running, run
`loginctl enable-linger` once.

## Set up the cloud project

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
just cloud-setup
```

The command checks each resource first, so you can run it again.

## Make the OAuth client

riff signs in with one Google OAuth client. Google has no API to make
it, so you make it by hand, once, in the console. The console can use
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
   just oauth-client
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

## Sign in

Without a sign-in, riff uses `USER` as your user. To sign in with
Google:

1. Install the server as a service, with the OAuth client:

   ```sh
   just service
   ```

   `just service` gets the client from `deploy/cloud.env` and Secret
   Manager. It gives them, and its own options, to
   `riff-server install`. For example: `just service --admin USER`.

   Only accounts of `comotechnologies.io` can sign in. To allow other
   Workspace domains, set `RIFF_ALLOWED_DOMAINS`, with commas between
   the domains.

2. Sign in. Your browser opens:

   ```sh
   riff login
   ```

3. Start your Claude Code sessions again. A session that started
   before `riff login` has no token.

The user part of your URI is now the part of your email before the
`@`. `riff logout` removes the sign-in from this device.
`riff logout --all` ends each of your sign-ins, on each device.

An admin can end each sign-in of another person. Name the admins when
you start the server, with `--admin USER` for each admin. Then an
admin runs `riff logout --all --user USER`.

Each token works only with the device key of this machine. `riff`
keeps the key in the OS keyring. Use the same server URL for `riff`
(`RIFF_SERVER`) as the server has for itself (`--public-url`, by
default `http://` and the listen address). Else the server refuses
each proof.

With `--require-sign-in`, the server refuses each call without a
token. `riff` sends a token on each call when you are signed in. A
command that you type acts as you. Each Claude Code session gets its
own token, which acts only as that session.
