# MyApps — Deployment

## Target Environment

- **Hardware**: Odroid N2 (4 GB RAM, ARM64)
- **OS**: Ubuntu Server 24.04 (aarch64)
- **Reverse proxy**: nginx + certbot (HTTPS)
- **Init system**: systemd
- **URL**: `https://yourdomain.com/myapps`

## Build Strategy

Release binaries are **cross-compiled** for `aarch64-unknown-linux-gnu` using
[`cross`](https://github.com/cross-rs/cross) plus `sccache`. Each merge to
`main` triggers GitHub Actions to bump the version, create a GitHub Release
with a tarball (`myapps-<tag>-aarch64.tar.gz`) containing the binary and
`static/` folder, and deploy it to staging then production.

The same pipeline runs locally from any x86_64 dev machine with Docker:
`make build-arm64` produces the aarch64 binary, `make deploy-stage` packages
and ships it to the Odroid via `deploy.sh release-deploy`. As a fallback,
`deploy.sh deploy` can still rsync source and build natively on the Odroid.

### Which tool does what

Three things can deploy, and they overlap deliberately rather than by accident:

| | Builds | Ships | Use when |
|---|---|---|---|
| **CD** (`.github/workflows/cd.yml`) | cross-compiles on a runner | `deploy.sh <env> release-deploy` | Always, for anything merged to `main` |
| **`make deploy-stage` / `deploy-prod`** | cross-compiles locally (Docker + `cross`) | the same `release-deploy` path | Trying a branch on staging, or a prod hotfix that cannot wait for CI |
| **`./deploy.sh <env> deploy`** | natively **on the Odroid** (slow, ~20 min) | `install` | Docker is unavailable, or the cross toolchain is broken |

The Makefile owns *building and packaging*; `deploy.sh` owns *everything that
touches the server*. The Makefile never talks to the server itself — every path
funnels into `deploy.sh`, so the install, systemd-unit and restart logic exists
in exactly one place. Server-side operations that have no build step (`setup`,
`restart`, `logs`, `status`) have no Makefile target on purpose; run
`./deploy.sh <env> <command>` for those.

`make deploy-prod` asks for confirmation: it bypasses the staging soak and the
smoke test that CD runs, and it installs a binary that no version tag points at,
so the next CD run will overwrite it with whatever is on `main`.

Neither Makefile target runs `make check` first. Run it yourself before shipping
a local build.

## Prerequisites

### Development machine (Linux or macOS)

- SSH access to the Odroid via the `deploy` user (key-based auth)
- `rsync`
- For local cross-compilation (recommended over on-device build):
  Docker (or Podman with `CROSS_CONTAINER_ENGINE=podman`),
  [`cross`](https://github.com/cross-rs/cross)
  (`cargo install cross --git https://github.com/cross-rs/cross`),
  and `sccache`. `make build-arm64` self-bootstraps a musl-static sccache
  binary into `~/.cache/cross-tools/` for use inside the cross container.

### Server (Odroid N2)

- nginx installed and running
- whisper.cpp + ffmpeg (for VoiceToText) — see [whisper.cpp section](#whispercpp-voicetotext) below
- llama.cpp server (for Command Bar) — see [llama.cpp section](#llamacpp-command-bar) below

#### Deploy user setup

All deployments (both manual and CI/CD) use a dedicated `deploy` user. This
keeps the Rust toolchain, build cache, and sudo permissions in one place, and
limits the blast radius of the SSH key stored in GitHub Secrets.

```bash
# On the Odroid — create the user
sudo useradd --system --create-home --shell /bin/bash deploy
sudo mkdir -p /home/deploy/.ssh
sudo chmod 700 /home/deploy/.ssh
sudo chown deploy:deploy /home/deploy/.ssh
```

Install the Rust toolchain and sccache:

```bash
sudo -u deploy bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
sudo -u deploy bash -c 'source ~/.cargo/env && cargo install sccache --locked'
```

Grant the sudo commands that `deploy.sh` needs. **Every entry must be
`NOPASSWD`.** `deploy.sh` pipes its remote scripts to `bash` over stdin, so the
remote `sudo` has no terminal to prompt on: a password-requiring rule does not
prompt, it fails the deploy. This is true of interactive runs as well as CI —
the `-t` flag `deploy.sh` passes outside CI cannot allocate a PTY when stdin is
a heredoc.

```bash
sudo visudo -f /etc/sudoers.d/deploy
```

```
# systemd unit is rewritten on every deploy (write_unit), not just at setup
deploy ALL=(ALL) NOPASSWD: \
    /usr/bin/systemctl restart myapps, \
    /usr/bin/systemctl restart myapps-stage, \
    /usr/bin/systemctl --no-pager status myapps, \
    /usr/bin/systemctl --no-pager status myapps-stage, \
    /usr/bin/systemctl daemon-reload, \
    /usr/bin/tee /etc/systemd/system/myapps.service, \
    /usr/bin/tee /etc/systemd/system/myapps-stage.service, \
    /usr/bin/journalctl *, \
    /usr/bin/mkdir *, \
    /usr/bin/sed *, \
    /usr/bin/cp *, \
    /usr/bin/mv *, \
    /usr/bin/chown *, \
    /usr/bin/chmod *, \
    /usr/bin/rsync *, \
    /usr/bin/sudo -u myapps *
```

Note what each group is for, so the list can be trimmed knowingly:

| Rule | Used by |
|------|---------|
| `systemctl restart` / `status` | `restart`, `status`, and the tail of every deploy |
| `systemctl daemon-reload`, `tee /etc/systemd/system/…` | `write_unit`, which runs on **every** deploy |
| `journalctl` | `logs` |
| `mkdir`, `sed`, `chown` | `write_unit` reading `.env` and preparing the FileClipboard directory |
| `cp`, `mv`, `chmod`, `rsync` | installing the binary and `static/` |
| `sudo -u myapps` | running CLI subcommands (`invite`, `create-user`, `cron`) as the service user |

`setup` needs considerably more than this — `useradd`, `apt-get`,
`tee` into `/etc/nginx/…` and `/etc/cron.d/…`, `nginx -t`, `systemctl reload
nginx`. Rather than widening the deploy user's rules permanently, run `setup`
once from an admin account with full sudo (`DEPLOY_SERVER=you@host ./deploy.sh
prod setup`, or the equivalent commands by hand), then hand routine deploys to
the restricted `deploy` user.

Generate a key pair and authorize it:

```bash
# On your dev machine
ssh-keygen -t ed25519 -C "myapps-deploy" -f ~/.ssh/myapps_deploy_key -N ""

# Copy the public key to the server (deploy user has no password, so use your
# existing sudo-capable user to place it)
cat ~/.ssh/myapps_deploy_key.pub | ssh youruser@odroid.local \
    'sudo tee /home/deploy/.ssh/authorized_keys > /dev/null && sudo chown deploy:deploy /home/deploy/.ssh/authorized_keys && sudo chmod 600 /home/deploy/.ssh/authorized_keys'
```

Configure your local SSH to use this key (add to `~/.ssh/config`):

```
Host odroid.local
    User deploy
    IdentityFile ~/.ssh/myapps_deploy_key
```

Set `DEPLOY_SERVER=deploy@odroid.local` in your `deploy/*.env` files.

**Use `user@host`, not a bare `~/.ssh/config` alias.** The same value is
uploaded to GitHub by `make gh-env`, and the CD workflow splits it on `@` to
build its own SSH config on the runner, where your local aliases do not exist.
An alias yields `User=myalias HostName=myalias` and every CD deploy fails to
connect. `make gh-env` refuses to run if any `deploy/*.env` has a
`DEPLOY_SERVER` without an `@`.

#### GitHub CD secrets

Upload the SSH key and known hosts to GitHub for CI/CD:

```bash
gh secret set SSH_PRIVATE_KEY < ~/.ssh/myapps_deploy_key
ssh-keyscan odroid.local | gh secret set SSH_KNOWN_HOSTS

# Create GitHub environments and set all variables from deploy/*.env
make gh-env
```

`make gh-env` reads each `deploy/*.env` file, creates the GitHub environment
(from `DEPLOY_GH_ENVIRONMENT`), and sets all non-empty variables. Empty values
are skipped — GitHub doesn't allow empty environment variables. It also asserts
that `DEPLOY_REMOTE_BUILD_DIR` is identical across all environments (required
because the CD pipeline builds once on staging and reuses the binary for
production).

## Quick Start

```bash
# 1. Set up the deploy user on the server (see "Deploy user setup" above)

# 2. Set DEPLOY_SERVER in your deploy/*.env files (user@host, e.g. deploy@odroid.local)

# 3. First-time server setup (creates myapps user, dirs, systemd, cron, nginx).
#    Needs full sudo, so run it as an admin account:
#    DEPLOY_SERVER=you@odroid.local ./deploy.sh prod setup
./deploy.sh prod setup

# 4. SSH into the server and edit /opt/myapps/.env with your values

# 5. Set up HTTPS on the server (as an admin account — the deploy user's sudo
#    rules do not cover apt or certbot)
ssh you@odroid.local 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d yourdomain.com'

# 6. Build locally and ship it (falls back to ./deploy.sh prod deploy, which
#    rsyncs source and compiles on the Odroid, if you have no Docker)
make deploy-prod

# 7. Create your first user (option A: invite link — user picks their own password)
ssh deploy@odroid.local 'sudo -u myapps /opt/myapps/myapps invite'
# Share the printed URL with the user

# 8. Create your first user (option B: direct — you choose the password)
ssh deploy@odroid.local 'sudo -u myapps /opt/myapps/myapps create-user --username yourname --password yourpass'
```

## deploy.sh Commands

Usage: `./deploy.sh <env> <command>`

| Command                      | Description                                              |
|------------------------------|----------------------------------------------------------|
| `release-deploy <dir>`      | Upload pre-built binary + static from a local directory, restart (used by CD and `make deploy-stage`) |
| `setup`                     | First-time server provisioning                           |
| `deploy`                    | Rsync source + build on server + install + restart       |
| `install`                   | Rsync source + install + restart (skip build)            |
| `build`                     | Rsync source + build on server (no install)              |
| `restart`                   | Restart the service                                      |
| `logs`                      | Tail the service logs (journalctl)                       |
| `status`                    | Show service status                                      |

Outside CI, `deploy.sh` multiplexes SSH (one TCP connection shared across the
many ssh/scp/rsync calls) via `ControlMaster=auto` to avoid tripping sshd
`MaxStartups` / fail2ban during a deploy. CI keeps a single connection per
step and skips multiplexing.

The `release-deploy` command is used by the CD pipeline — it takes a directory
(extracted from the release tarball) containing the binary and `static/` folder,
and copies them to the target directory (`DEPLOY_REMOTE_DIR`) via SCP/rsync,
without needing a build directory on the server. The `deploy` and `install`
commands are kept for local manual deploys.

Available environments are defined by config files in `deploy/`:

| Environment | Config file      | URL                                      | Port |
|-------------|------------------|------------------------------------------|------|
| `prod`      | `deploy/prod.env` | `https://yourdomain.com`          | 3000 |
| `stage`     | `deploy/stage.env` | `https://stage.yourdomain.com`    | 3001 |

The SSH target is set via `DEPLOY_SERVER` in each `deploy/*.env` file, as
`user@host` (e.g. `deploy@odroid.local`) — see the note under
[Deploy user setup](#deploy-user-setup) for why an alias will not do.

## Deploy Flow

### CD pipeline (automatic, on merge to main)

```
GitHub Actions                      Odroid N2
──────────────                      ─────────
push to main
  │
  ├─ bump version in Cargo.toml
  ├─ commit + tag (v0.2.0)
  ├─ cross build --target aarch64
  ├─ package tarball (binary + static/)
  ├─ create GitHub Release
  │
  ├─ [deploy-stage]
  │    ├─ gh release download tarball
  │    ├─ extract + scp binary + static ──▸  /opt/myapps-stage/
  │    ├─ ssh: restart
  │    └─ smoke test /login → 200
  │
  └─ [deploy-prod]
       ├─ gh release download tarball
       ├─ extract + scp binary + static ──▸  /opt/myapps/
       ├─ ssh: restart
       └─ smoke test /login → 200
```

### Manual deploy (from dev machine)

Preferred path — local cross-compile, ship binary:

```
Dev machine                              Odroid N2
───────────                              ─────────
make deploy-stage           (or deploy-prod)
  │
  ├─ cross build --target aarch64    (Docker + sccache cache)
  ├─ package release-pkg/ (binary + static/)
  │
  ├─ scp binary  ──────────────────▸  /opt/myapps-stage/myapps
  ├─ rsync static ─────────────────▸  /opt/myapps-stage/static/
  ├─ ssh: sudo systemctl restart       └─ service running
  │
  └─ done
```

Fallback — rsync source, build on the Odroid (slow, kept for emergencies):

```
Dev machine                         Odroid N2
───────────                         ─────────
./deploy.sh prod deploy
  │
  ├─ rsync source ──────────────▸  ~/myapps-build/
  │                                  │
  │  ssh: cargo build --release      ├─ compile natively
  │                                  │
  │  ssh: sudo cp binary             ├─ /opt/myapps/myapps
  │                                  │
  │  ssh: sudo systemctl restart     └─ service running
  │
  └─ done
```

## What `setup` Does

Run once on a fresh server. It:

1. Installs the Rust toolchain (if not already present)
2. Runs `apt-get update`, installs build dependencies (`pkg-config`,
   `libssl-dev`, `sqlite3`) and `sccache`
3. Creates a `myapps` system user (no login shell)
4. Creates `$DEPLOY_REMOTE_DIR/{data,static}` and the FileClipboard storage
   directory, with proper ownership
5. Creates `$DEPLOY_REMOTE_DIR/.env` template (chmod 600)
6. Installs the systemd unit for the environment (also refreshed on every
   deploy — see below)
7. Installs a cron job for daily scheduled tasks at 06:00 (if `DEPLOY_CRON_ENABLED=true`)
8. Installs an nginx site config for the configured domain — **only if one does
   not already exist.** When it does, `setup` leaves it alone and warns if
   `client_max_body_size` or `proxy_request_buffering` is missing from it (the
   two directives FileClipboard uploads need).

The `.env` it writes seeds `DEPLOY_APPS`, `SEED`, `AUTH_SSO_HEADER` and
`EXTERNAL_APPS` from the matching `DEPLOY_*` values in `deploy/<env>.env`.
Everything else — `ENCRYPTION_KEY`, the VAPID keys, `LLAMA_SERVER_URL` — is left
blank for you to fill in.

`setup` is idempotent in the parts that matter (user, directories, cron, systemd
unit) but never overwrites an existing `.env` or nginx site. Re-running it on a
live server is safe.

After setup, enable HTTPS with certbot (see Quick Start step 5).

## Directory Structure on Server

```
/opt/myapps/               # Runtime (owned by myapps user)
├── myapps                 # Binary
├── .env                   # Environment variables (chmod 600)
├── private.pem            # Enable Banking RSA private key (chmod 600)
├── data/
│   └── myapps.db          # SQLite database (created on first run)
└── static/                # Static assets (CSS, icons), synced on every deploy

~/myapps-build/            # Build directory (owned by deploy user)
├── src/
├── Cargo.toml
├── Cargo.lock
└── target/                # Compilation artifacts (cached between deploys)
```

The build directory (`~/myapps-build`) is separate from the runtime directory
(`/opt/myapps`). Cargo's `target/` is cached on the server, so subsequent
builds are incremental and fast.

## Environment Variables

File: `/opt/myapps/.env`

Adding or removing a variable means touching five files: `.env.example`,
`deploy/*.env.example`, the `.env` template in `deploy.sh` (`setup()`), the
"Generate deploy config" heredoc in `.github/workflows/cd.yml` (for `DEPLOY_*`
variables), and the table below. A variable missing from one of them fails at
runtime, not at build time.

```bash
DATABASE_URL=sqlite:///opt/myapps/data/myapps.db
BASE_URL=https://yourdomain.com                           # Public URL
ENCRYPTION_KEY=                                           # 32-byte hex (openssl rand -hex 32)
VAPID_PRIVATE_KEY=                                        # base64url-encoded EC private key
VAPID_PUBLIC_KEY=                                         # base64url-encoded uncompressed public key
VAPID_SUBJECT=mailto:you@example.com                      # VAPID subject claim
WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli   # whisper.cpp binary
WHISPER_MODELS_DIR=/opt/whisper.cpp/models                # GGML model directory
LLAMA_SERVER_URL=                                         # llama.cpp server URL (optional)
FILE_CLIPBOARD_DIR=/opt/myapps/data/file_clipboard        # FileClipboard upload directory
FILE_CLIPBOARD_RETENTION_DAYS=7                           # Default deletion period for new uploads
FILE_CLIPBOARD_MAX_FILE_BYTES=5368709120                  # Largest single upload (default 5 GiB)
FILE_CLIPBOARD_USER_QUOTA_BYTES=21474836480               # Per-user total (default 20 GiB)
FILE_CLIPBOARD_MIN_FREE_BYTES=2147483648                  # Reject uploads below this free space (default 2 GiB)
BIND_ADDR=127.0.0.1:3000
DEPLOY_APPS=                                              # Comma-separated app keys (blank = all)
AUTH_SSO_HEADER=                                          # Trusted SSO header (e.g. Remote-User for Authelia)
EXTERNAL_APPS=                                            # External app shortcuts (key|name|desc|icon|url;...)
SEED=false                                                # Auto-seed on invite registration (true/false)
CLEANUP_INACTIVE_DAYS=0                                   # Delete inactive users after N days (0 = off)
```

`LLAMA_SERVER_URL` enables the command bar (natural language command entry).
When set, myapps sends requests to a running llama.cpp server
(`llama-server --port 8081 -m model.gguf`). When empty the command bar is hidden.

Only `DATABASE_URL` and `BIND_ADDR` are required to start the server.
`DEPLOY_APPS` limits which apps are mounted and shown in the launcher. Valid
keys: `leanfin`, `mindflow`, `voice_to_text`, `form_input`, `notes`,
`file_clipboard`. When empty or unset, all apps are available.
`AUTH_SSO_HEADER` enables reverse-proxy SSO authentication (e.g. Authelia). When
set to the header name that carries the authenticated username (typically
`Remote-User`), myapps trusts that header and auto-creates users on first visit.
The login page is bypassed. When empty, only username/password login is used.
Ensure your reverse proxy strips client-sent values for this header before
setting the authenticated value.
`EXTERNAL_APPS` adds shortcut tiles to the launcher that open external services
in a new tab. Format: `key|name|description|icon|url` entries separated by `;`.
Example: `vault|Vaultwarden|Password manager|🔐|https://vault.example.com`.
`BASE_URL` is the public URL of the application. `ENCRYPTION_KEY` is needed for
storing Enable Banking credentials (per-user encrypted settings).

### FileClipboard storage

FileClipboard stores file *contents* on disk, not in SQLite, so its directory
shares a filesystem with `myapps.db` unless you move it. A full disk does not
just fail uploads — it fails SQLite writes for every other app — so:

- Point `FILE_CLIPBOARD_DIR` at a separate mount (e.g. an external SSD) on any
  deployment where people will store more than a few gigabytes. The deploy
  config knob is `DEPLOY_FILE_CLIPBOARD_DIR` in `deploy/{stage,prod}.env`.
- Keep `FILE_CLIPBOARD_MIN_FREE_BYTES` well above zero. Uploads abort mid-stream
  once free space would drop below it, leaving headroom for the database.
- `FILE_CLIPBOARD_MAX_FILE_BYTES` must not exceed nginx's `client_max_body_size`
  (see below) — nginx rejects an oversized body before the app ever sees it.

**A storage directory outside the deploy directory needs a systemd exception.**
The service runs with `ProtectSystem=strict`, which mounts the whole filesystem
read-only in its private mount namespace and punches back through only the paths
listed in `ReadWritePaths`. A directory that is not listed fails with
`Read-only file system (os error 30)` however it is owned — `chown` and `chmod`
have no effect, and the same path stays writable from a shell, which makes this
look like an application bug rather than a sandbox.

`write_unit()` in `deploy.sh` handles this, and it runs on **every deploy**
(`deploy`, `install`, `release-deploy`) as well as on `setup`. It reads
`FILE_CLIPBOARD_DIR` from the server's own `.env` — the value the app actually
uses — and when that path is outside the deploy directory it adds it to
`ReadWritePaths` and emits `RequiresMountsFor`, so the service waits for an
external disk instead of racing it at boot. It prints the storage directory and
the resulting `ReadWritePaths` as it goes.

So after changing `FILE_CLIPBOARD_DIR` in the server's `.env`, a normal deploy
is enough:

```bash
make deploy-stage        # or ./deploy.sh stage deploy
```

To patch a running server without deploying (note the **service name** differs
per environment — `myapps` vs `myapps-stage`):

```bash
sudo systemctl edit myapps-stage   # [Service] / ReadWritePaths=/mnt/hdd/myapps
sudo systemctl restart myapps-stage
```

Drop-ins created with `systemctl edit` live in `<service>.service.d/` and
survive the unit being rewritten. Direct edits to the unit file itself do not.

The server logs which case it is at startup — either `storage directory <dir> is
writable` or an error naming the fix — visible with
`journalctl -u myapps-stage -n 50`.

Expired files and orphaned bytes are removed by a sweep that runs on the daily
cron job *and* every six hours inside the server process, so retention still
works on deployments where `DEPLOY_CRON_ENABLED=false`.

## systemd Service

Installed at `/etc/systemd/system/myapps.service` by `write_unit()` in
`deploy.sh`, which runs on **every** deploy (`deploy`, `install`,
`release-deploy`) as well as on `setup`. The unit is therefore declarative: edit
`deploy.sh` and the next deploy applies it. The corollary is that hand-edits to
the unit file on the server are overwritten — use `systemctl edit <service>` for
local overrides, which live in a separate `<service>.service.d/` directory and
survive.

```bash
sudo systemctl enable myapps    # auto-start on boot
sudo systemctl start myapps
sudo systemctl status myapps
sudo journalctl -u myapps -f    # tail logs
```

## Cron Job

Installed at `/etc/cron.d/myapps` by `setup`. Runs daily at 06:00:

```
0 6 * * * myapps /opt/myapps/myapps cron
```

## Backups and Rollback

Nothing in the deploy path backs anything up, and there is no automatic
rollback. Both are manual, and worth knowing before you need them.

**Migrations run on startup, on every deploy.** `db::migrator()` merges core and
per-app migrations by timestamp and applies whatever is pending the moment the
service comes up. There is no confirmation step and no backup step. Take one
first whenever a release contains a migration:

```bash
# Consistent copy even while the service is running (WAL mode)
ssh deploy@odroid.local \
    'sudo -u myapps sqlite3 /opt/myapps/data/myapps.db ".backup /tmp/myapps-$(date +%F).db"'
scp deploy@odroid.local:/tmp/myapps-*.db ./backups/
```

Copying `myapps.db` with `cp`/`rsync` while the service runs is not safe — the
`-wal` and `-shm` files hold committed data that the main file does not.

**Rolling back the binary** means redeploying an earlier release tarball:

```bash
gh release download v0.4.1 --pattern 'myapps-*.tar.gz' --dir /tmp/rollback
mkdir -p /tmp/rollback/pkg && tar -xzf /tmp/rollback/myapps-*.tar.gz -C /tmp/rollback/pkg
./deploy.sh prod release-deploy /tmp/rollback/pkg
```

`release-deploy` overwrites the installed binary in place and keeps no copy of
the previous one, so the release tarball is the only artifact to roll back to.

**A binary rollback does not roll back the schema.** Migrations are
forward-only; an older binary runs against the newer schema. Restore the
database backup alongside the binary if the release migrated anything
destructive.

**FileClipboard contents are not in the database.** A database backup restores
file *metadata* only; the bytes live under `FILE_CLIPBOARD_DIR`. Restoring one
without the other leaves `services::retention` to reconcile the difference — it
deletes disk files with no matching row.

## Web Push Notifications

The app uses the standard Web Push API with VAPID authentication for browser
push notifications. No separate notification service is needed — the app
sends push messages directly to browser push endpoints.

### Generate VAPID keys

```bash
# On the server (or locally)
/opt/myapps/myapps generate-vapid-keys
```

This prints a key pair. Add the output to `/opt/myapps/.env`:

```bash
VAPID_PRIVATE_KEY=<generated private key>
VAPID_PUBLIC_KEY=<generated public key>
VAPID_SUBJECT=mailto:you@example.com
```

Restart the service after updating `.env`.

### Enable notifications

Open the app in a browser, navigate to the launcher page, and click
"Enable notifications". The browser will prompt for permission. Once granted,
the subscription is stored in the database and push notifications will be
delivered to that browser.

### Supported platforms

- **Desktop**: Chrome, Firefox, Edge, Safari 16+
- **Android**: Chrome (including installed PWA)
- **iOS**: Safari 16.4+ (requires the app to be installed as a PWA via
  "Add to Home Screen")

## whisper.cpp (VoiceToText)

whisper.cpp is the speech-to-text engine used by the VoiceToText app. It runs
entirely on CPU using ARM NEON SIMD — no GPU or NPU required.

### Install build dependencies

```bash
sudo apt install -y build-essential cmake ffmpeg
```

ffmpeg is needed to convert uploaded audio to the 16 kHz mono WAV format that
whisper.cpp expects.

### Build whisper.cpp

```bash
cd /opt
sudo git clone https://github.com/ggml-org/whisper.cpp.git
cd whisper.cpp
sudo cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
sudo cmake --build build -j4
```

The binary will be at `/opt/whisper.cpp/build/bin/whisper-cli`.

### Download models

```bash
cd /opt/whisper.cpp

# Base model (recommended — good accuracy, ~1-2 min per minute of audio)
sudo ./models/download-ggml-model.sh base

# Tiny model (optional — faster, less accurate, ~30-60s per minute of audio)
sudo ./models/download-ggml-model.sh tiny
```

Model sizes on disk: tiny ~75 MB, base ~142 MB. At runtime they use roughly
2x their disk size in RAM.

### Configure MyApps

Add to `/opt/myapps/.env`:

```bash
WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli
WHISPER_MODELS_DIR=/opt/whisper.cpp/models
```

Both have defaults (`whisper-cli` and `models` respectively), so if you symlink
the binary into `$PATH` and keep models in a `models/` directory relative to the
working dir, you can skip these.

### Verify

```bash
# Test transcription with a sample file
/opt/whisper.cpp/build/bin/whisper-cli \
    -m /opt/whisper.cpp/models/ggml-base.bin \
    -f /opt/whisper.cpp/samples/jfk.wav \
    --no-timestamps
```

### Performance on Odroid N2

| Model | RAM at runtime | ~Time per 1 min audio | Notes |
|-------|---------------|----------------------|-------|
| tiny  | ~200 MB       | 30–60s               | Near real-time |
| base  | ~400 MB       | 60–120s              | Recommended for async use |
| small | ~1.2 GB       | 3–5 min              | Feasible but slow |

The background worker processes one job at a time to avoid memory pressure.
With 4 GB RAM, tiny and base fit comfortably alongside the Axum server.

## llama.cpp (Command Bar)

llama.cpp powers the natural-language command bar. It runs as a persistent HTTP
server so the model stays loaded in memory between requests.

### Install build dependencies

```bash
sudo apt install -y build-essential cmake
```

### Build llama.cpp

```bash
cd /opt
sudo git clone https://github.com/ggml-org/llama.cpp.git
cd llama.cpp
sudo cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
sudo cmake --build build -j4
```

The server binary will be at `/opt/llama.cpp/build/bin/llama-server`.

### Download a model

Any small instruction-tuned GGUF model works. Qwen2.5-1.5B-Instruct is
recommended — it's a pure transformer where all layers use KV cache, enabling
effective prompt prefix caching. Hybrid models like Qwen3.5 use SSM layers that
must re-evaluate the full sequence on every request, making caching ineffective.

```bash
sudo mkdir -p /opt/llama.cpp/models
cd /opt/llama.cpp/models
sudo wget https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q5_k_m.gguf
```

Model size: ~1.2 GB on disk, ~1.5 GB RAM at runtime. With whisper base loaded,
total memory use stays under 3 GB. Qwen2.5 uses ChatML natively, matching how
MyApps constructs prompts.

### Install as a systemd service

```bash
sudo tee /etc/systemd/system/llama-server.service > /dev/null <<'SERVICE'
[Unit]
Description=llama.cpp inference server
After=network.target

[Service]
Type=simple
ExecStart=/opt/llama.cpp/build/bin/llama-server \
    --host 127.0.0.1 \
    --port 8081 \
    -m /opt/llama.cpp/models/qwen2.5-1.5b-instruct-q5_k_m.gguf \
    -c 2048 \
    --parallel 1
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
SERVICE

sudo systemctl daemon-reload
sudo systemctl enable llama-server
sudo systemctl start llama-server
```

### Configure MyApps

Add to `/opt/myapps/.env`:

```bash
LLAMA_SERVER_URL=http://127.0.0.1:8081
```

Restart myapps after updating `.env`. The command bar will appear at the bottom
of every page.

### Verify

```bash
# Check the server is running
curl http://127.0.0.1:8081/health

# Test a completion
curl http://127.0.0.1:8081/completion \
    -H "Content-Type: application/json" \
    -d '{"prompt":"Say hello","cache_prompt":true,"id_slot":0,"n_predict":32}'
```

### Performance on Odroid N2

| Model | RAM | ~Inference time | Notes |
|-------|-----|----------------|-------|
| Qwen2.5-1.5B-Instruct Q5_K_M | ~1.5 GB | 1–7s (cached) | Recommended — pure transformer, cache-friendly |
| Qwen3.5-2B Q4_K_M | ~1.5 GB | 3–6s | Hybrid SSM, limited cache benefit |
| SmolLM3-3B Q4_K_M | ~2.0 GB | 4–8s | More capable, higher RAM |
| Gemma 3 1B-it Q4_K_M | ~0.7 GB | 1–3s | Fastest, less accurate |

The server processes one request at a time. MyApps uses a mutex to serialize
command requests so the server is never overloaded.

## CI/CD Pipeline

Merging to `main` triggers automatic deployment via `.github/workflows/cd.yml`:

```
push to main
    │
    ▼
 [release]       ◄── auto-bump version, cross-compile aarch64, package tarball, create GitHub Release
    │
    ▼
 [deploy-stage]  ◄── download release tarball, extract, upload to server, install + restart
    │ smoke test /login → 200
    ▼
 [deploy-prod]   ◄── download same release tarball, extract, upload to server, install + restart
    │ smoke test /login → 200
    ▼
  Done
```

### Versioning

The version in `Cargo.toml` is bumped during development as part of the
`/finish-development` workflow, before the PR is opened. Bump type is
determined by the branch name and commit prefixes:

| Prefix        | Bump  | Example                          |
|---------------|-------|----------------------------------|
| `[FEAT-*]`   | minor | `[FEAT-42] Add new dashboard`    |
| `[BREAKING-*]`| major | `[BREAKING] Remove legacy API`  |
| anything else | patch | `[BUG-99] Fix login redirect`    |

Makefile targets are available for manual use: `make bump-patch`,
`make bump-minor`, `make bump-major`.

When merged to `main`, the CD pipeline reads the version from `Cargo.toml`,
creates a git tag (`v0.2.0`), and publishes a GitHub Release with a tarball
containing the binary and static assets. If the tag already exists (e.g.
re-running the workflow), the release step is skipped and the existing
release tarball is deployed.

CI (format, clippy, tests) runs separately via `ci.yml`. The CD pipeline
trusts that CI has already passed on `main`.

### GitHub configuration

The CD workflow requires two GitHub **Environments** (`staging` and
`production`), each with the following configuration:

**Secrets** (repo-level or per-environment):

| Secret             | Description                                         |
|--------------------|-----------------------------------------------------|
| `SSH_PRIVATE_KEY`  | Ed25519 private key authorized on the server        |
| `SSH_KNOWN_HOSTS`  | Output of `ssh-keyscan -p <port> <server-host>`     |

**Environment variables** (per GitHub Environment):

| Variable                  | Example (staging)           | Example (production)       |
|---------------------------|-----------------------------|----------------------------|
| `DEPLOY_SERVER`           | `deploy@odroid.local`       | `deploy@odroid.local`      |
| `DEPLOY_SSH_PORT`         | `22`                        | `22`                       |
| `DEPLOY_DOMAIN`           | `stage.yourdomain.com`      | `yourdomain.com`           |
| `DEPLOY_REMOTE_DIR`       | `/opt/myapps-stage`         | `/opt/myapps`              |
| `DEPLOY_REMOTE_BUILD_DIR` | `~/myapps-stage-build`      | `~/myapps-build`           |
| `DEPLOY_SERVICE_NAME`     | `myapps-stage`              | `myapps`                   |
| `DEPLOY_NGINX_SITE`       | `myapps-stage`              | `myapps`                   |
| `DEPLOY_PORT`             | `3001`                      | `3000`                     |
| `DEPLOY_CRON_ENABLED`     | `false`                     | `true`                     |
| `DEPLOY_ICON`             | `icon-stage.svg`            | `icon.svg`                 |
| `DEPLOY_SEED`             | `true`                      | `false`                    |
| `DEPLOY_FILE_CLIPBOARD_DIR` | *(blank, or e.g. `/mnt/data/file_clipboard`)* | *(same)* |

These match the values in `deploy/*.env.example`, and `make gh-env` uploads them
from your local `deploy/*.env`. Adding a new `DEPLOY_*` variable means adding it
to the "Generate deploy config" heredoc in `cd.yml` as well — `make gh-env` will
happily upload a variable the workflow never writes into the env file, and
`deploy.sh` then falls back to its default without complaining.

### Server prerequisites for CI/CD

The same `deploy` user is used for both manual and CI/CD deploys. The
`DEPLOY_CI=true` flag tells `deploy.sh` to skip `-t` (TTY allocation) since
CI runners have no interactive terminal.

### Manual trigger

The CD workflow supports `workflow_dispatch`, so you can trigger a deploy
manually from the GitHub Actions UI without pushing a commit. Manual runs
deploy to staging by default; tick the **Also deploy to production** input
to continue on to prod after staging.

## nginx + HTTPS

The generated config sets `client_max_body_size 5g` and
`proxy_request_buffering off` for FileClipboard uploads. **`setup` only writes
the site config when one does not already exist** — unlike the systemd unit, an
nginx config is never rewritten, so changes here do not reach servers that were
already set up. `setup` warns when an existing config is missing either
directive; add them by hand and reload:

```bash
sudo nginx -t && sudo systemctl reload nginx
```

Without them nginx rejects uploads over its 1 MB default, before the app sees
the request. `client_max_body_size` must also stay at or above
`FILE_CLIPBOARD_MAX_FILE_BYTES` — the two are set independently and nothing
checks that they agree.

The `setup` command installs an HTTP-only nginx config at
`/etc/nginx/sites-available/myapps` with `server_name` set to your domain.
The config proxies all requests to `127.0.0.1:3000`.

To enable HTTPS:

```bash
sudo apt install python3-certbot-nginx
sudo certbot --nginx -d yourdomain.com
```

Certbot will modify the nginx config to add `listen 443 ssl` with the
certificate paths and redirect HTTP to HTTPS automatically.

The app is then accessible at `https://yourdomain.com`.

## Staging Environment

A staging instance runs alongside production on the same Odroid, at
`https://stage.yourdomain.com`. It uses a separate database, systemd
service, and nginx site, listening on port 3001.

### Deploy config files

All environment-specific values live in `deploy/*.env`. The deploy script is
environment-agnostic — it sources the config file matching the first argument.

To add a new environment (e.g. `demo`), create `deploy/demo.env` with the
appropriate values.

### Setting up staging

```bash
# 1. First-time setup (creates dirs, systemd, nginx on the server)
./deploy.sh stage setup

# 2. Edit /opt/myapps-stage/.env on the server with appropriate values

# 3. DNS: add stage.yourdomain.com to your DNS provider

# 4. HTTPS
ssh you@odroid.local 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d stage.yourdomain.com'

# 5. Deploy
./deploy.sh stage deploy

# 6. Create a user (invite link or direct)
ssh deploy@odroid.local 'sudo -u myapps /opt/myapps-stage/myapps invite'
# Or: ssh deploy@odroid.local 'sudo -u myapps /opt/myapps-stage/myapps create-user --username yourname --password yourpass'
```

### Auto-seeding and user cleanup

When `SEED=true` is set in the server's `.env`, new users who register via an
invite link will automatically get demo data seeded for all deployed apps.

When `CLEANUP_INACTIVE_DAYS` is set (e.g. `7`), inactive users are
automatically cleaned up on each service start (i.e. on every deploy, since
the service restarts). You can also run it manually:

```bash
cargo run -- cleanup-users --days 7
```

### Directory structure (staging)

```
/opt/myapps-stage/         # Runtime (owned by myapps user)
├── myapps                 # Binary
├── .env                   # Environment variables (chmod 600)
├── data/
│   └── myapps.db          # SQLite database (separate from prod)
└── static/

~/myapps-stage-build/      # Build directory (owned by deploy user)
```
