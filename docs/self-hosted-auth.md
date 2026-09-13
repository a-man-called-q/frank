# Self-hosted Frank authentication

This milestone runs `frankd` with one local owner account. There is no signup,
Supabase project, email provider, or external identity service. The account is
created locally and the server stores only an Argon2id password hash and
revocable session digests.

## Start the server with Docker

Generate a development certificate for local use:

```sh
./scripts/dev-cert.sh
```

The generated certificate is valid for `localhost`, `127.0.0.1`, and `::1`.
For a LAN or domain deployment, mount a certificate whose SAN contains the
hostname the Flutter client will use.

Bootstrap the owner before starting the daemon. The command prompts for the
username and password without accepting either password value as an argument:

```sh
docker compose run --rm --no-deps frankd auth init \
  --db /var/lib/frank/frank.sqlite3
docker compose up -d
```

The SQLite database and server audit data live in the `frank-data` named
volume. The default port is published only on `127.0.0.1:37465`; set
`FRANK_PUBLISH_HOST=0.0.0.0` in the shell environment when a trusted LAN
client must connect. Keep TLS enabled when changing the bind or publish host.

To reset the password, stop the daemon, run the one-off command, and start it
again. Resetting the password revokes every session:

```sh
docker compose stop frankd
docker compose run --rm --no-deps frankd auth reset-password \
  --db /var/lib/frank/frank.sqlite3
docker compose up -d
```

Use `auth revoke-sessions` when a password change is not needed. Pairing
tickets and the old device-token path are disabled after the auth migration;
existing projects and tasks remain in the database, but clients must log in
again.

## Point the Flutter client at the server

Copy the tracked example and add the development certificate as a public trust
anchor when it is not already trusted by the operating system:

```sh
cd apps/frank_desktop
cp .env.example .env
export CERT_B64="$(base64 < ../../docker/tls/cert.pem | tr -d '\n')"
python3 -c 'import json, os; p=json.load(open(".env")); p["FRANK_SERVER_CA_PEM_BASE64"]=os.environ["CERT_B64"]; json.dump(p, open(".env", "w"), indent=2); print()'
```

`.env` is JSON because Flutter's `--dart-define-from-file` format is JSON,
even when the file is named `.env`. Run or build with the configured Moon
tasks:

```sh
moon run frank-desktop:run-configured --interactive
moon run frank-desktop:build-configured
```

The server URL must be HTTPS and cannot contain userinfo, query parameters, or
fragments. The CA value contains only a public certificate; never put a private
key, password, or session token in `.env`. Changing servers requires another
build/run with a new compile-time configuration.

To run the isolated Docker authentication smoke test (requires Docker, curl,
Python, and an unused local port `37465`), use the Moon task:

```sh
moon run frank-scrutiny:auth-docker-smoke
```

After bootstrap, log in with the owner username and password. The account menu
supports password changes, logout, and logout from all devices. A server restart
does not invalidate an unexpired session; logout-all and password changes do.

For a local macOS demo, `moon run demo:run-mac --interactive` starts an isolated
Compose project, creates or reuses `apps/frank_desktop/.env` with the local
development certificate, prompts for the owner on first use, and launches the
Flutter client. Stop it with `moon run demo:stop-mac`; the named volume is kept
so the owner and demo data remain available.

## Scope of this milestone

Login, session validation, logout, password reset, TLS, Docker health checks,
and the owner account lifecycle use the real Rust server. Office, project,
ledger, team, and agent surfaces still render fixture data and show the
permanent banner **“Demo data — fitur belum terhubung ke server”**. Provider
agent runtimes, billing, and SaaS identity management are later milestones.
