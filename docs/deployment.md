# MyApps — Deployment

## Target Environment

- **Hardware**: Odroid N2 (4 GB RAM, ARM64)
- **OS**: Ubuntu Server 24.04 (aarch64)
- **Reverse proxy**: nginx + certbot (HTTPS)
- **Init system**: systemd
- **URL**: `https://yourdomain.com/myapps`

## Build Strategy

Release binaries are **cross-compiled in GitHub Actions** for
`aarch64-unknown-linux-gnu` using [`cross`](https://github.com/cross-rs/cross).
Each merge to `main` automatically bumps the version, creates a GitHub Release
with the binary attached, and deploys it to staging then production.

For local development deploys, `deploy.sh deploy` can still build natively on
the Odroid if needed.

## Prerequisites

### Development machine (macOS)

- SSH access to the Odroid via the `deploy` user (key-based auth)
- `rsync` (pre-installed on macOS)

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

Grant only the sudo commands that `deploy.sh` needs:

```bash
sudo visudo -f /etc/sudoers.d/deploy
```

```
deploy ALL=(ALL) NOPASSWD: \
    /usr/bin/systemctl restart myapps, \
    /usr/bin/systemctl restart myapps-stage, \
    /usr/bin/systemctl --no-pager status myapps, \
    /usr/bin/systemctl --no-pager status myapps-stage, \
    /usr/bin/cp *, \
    /usr/bin/mv *, \
    /usr/bin/chown *, \
    /usr/bin/chmod *, \
    /usr/bin/rsync *, \
    /usr/bin/sudo -u myapps *
```

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
Host odroid-deploy
    HostName odroid.local
    User deploy
    IdentityFile ~/.ssh/myapps_deploy_key
```

Set `DEPLOY_SERVER=odroid-deploy` in your `deploy/*.env` files.

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

# 2. Set DEPLOY_SERVER in your deploy/*.env files (e.g. odroid-deploy)

# 3. First-time server setup (creates myapps user, dirs, systemd, cron, nginx)
./deploy.sh prod setup

# 4. SSH into the server and edit /opt/myapps/.env with your values

# 5. Set up HTTPS on the server
ssh odroid-deploy 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d yourdomain.com'

# 6. Sync source, build on server, install, and start the service
./deploy.sh prod deploy

# 7. Create your first user (option A: invite link — user picks their own password)
ssh odroid-deploy 'sudo -u myapps /opt/myapps/myapps invite'
# Share the printed URL with the user

# 7. Create your first user (option B: direct — you choose the password)
ssh odroid-deploy 'sudo -u myapps /opt/myapps/myapps create-user --username yourname --password yourpass'
```

## deploy.sh Commands

Usage: `./deploy.sh <env> <command>`

| Command                      | Description                                              |
|------------------------------|----------------------------------------------------------|
| `release-deploy <binary>`   | Upload a pre-built binary + static files directly to target dir, restart (used by CD) |
| `setup`                     | First-time server provisioning                           |
| `deploy`                    | Rsync source + build on server + install + restart       |
| `install`                   | Rsync source + install + restart (skip build)            |
| `build`                     | Rsync source + build on server (no install)              |
| `restart`                   | Restart the service                                      |
| `logs`                      | Tail the service logs (journalctl)                       |
| `status`                    | Show service status                                      |

The `release-deploy` command is used by the CD pipeline — it copies the
cross-compiled binary and static files directly to the target directory
(`DEPLOY_REMOTE_DIR`) via SCP/rsync, without needing a build directory on the
server. The `deploy` and `install` commands are kept for local manual deploys.

Available environments are defined by config files in `deploy/`:

| Environment | Config file      | URL                                      | Port |
|-------------|------------------|------------------------------------------|------|
| `prod`      | `deploy/prod.env` | `https://yourdomain.com`          | 3000 |
| `stage`     | `deploy/stage.env` | `https://stage.yourdomain.com`    | 3001 |

The SSH target is set via `DEPLOY_SERVER` in each `deploy/*.env` file
(e.g. `odroid-deploy` matching your SSH config alias).

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
  ├─ create GitHub Release
  │
  ├─ [deploy-stage]
  │    ├─ gh release download
  │    ├─ scp binary + static ──▸  /opt/myapps-stage/
  │    ├─ ssh: restart
  │    └─ smoke test /login → 200
  │
  └─ [deploy-prod]
       ├─ gh release download
       ├─ scp binary + static ──▸  /opt/myapps/
       ├─ ssh: restart
       └─ smoke test /login → 200
```

### Manual deploy (from dev machine)

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
2. Installs build dependencies (`pkg-config`, `libssl-dev`) and `sccache`
3. Creates a `myapps` system user (no login shell)
4. Creates `$DEPLOY_REMOTE_DIR/{data,logs,static}` with proper ownership
5. Creates `$DEPLOY_REMOTE_DIR/.env` template (chmod 600)
6. Installs the systemd unit for the environment
7. Installs a cron job for daily scheduled tasks at 06:00 (if `DEPLOY_CRON_ENABLED=true`)
8. Installs an nginx site config for the configured domain

After setup, enable HTTPS with certbot (see Quick Start step 4).

## Directory Structure on Server

```
/opt/myapps/               # Runtime (owned by myapps user)
├── myapps                 # Binary
├── .env                   # Environment variables (chmod 600)
├── private.pem            # Enable Banking RSA private key (chmod 600)
├── data/
│   └── myapps.db          # SQLite database (created on first run)
├── logs/
│   └── cron.log           # Cron job output
└── static/                # (reserved for future use)

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
BIND_ADDR=127.0.0.1:3000
DEPLOY_APPS=                                              # Comma-separated app keys (blank = all)
SEED=false                                                # Auto-seed on invite registration (true/false)
CLEANUP_INACTIVE_DAYS=0                                   # Delete inactive users after N days (0 = off)
```

`LLAMA_SERVER_URL` enables the command bar (natural language command entry).
When set, myapps sends requests to a running llama.cpp server
(`llama-server --port 8081 -m model.gguf`). When empty the command bar is hidden.

Only `DATABASE_URL` and `BIND_ADDR` are required to start the server.
`DEPLOY_APPS` limits which apps are mounted and shown in the launcher. Valid
keys: `leanfin`, `mindflow`, `voice_to_text`, `classroom_input`. When empty or
unset, all apps are available.
`BASE_URL` is the public URL of the application. `ENCRYPTION_KEY` is needed for
storing Enable Banking credentials (per-user encrypted settings).

## systemd Service

Installed at `/etc/systemd/system/myapps.service` by `setup`.

```bash
sudo systemctl enable myapps    # auto-start on boot
sudo systemctl start myapps
sudo systemctl status myapps
sudo journalctl -u myapps -f    # tail logs
```

## Cron Job

Installed at `/etc/cron.d/myapps` by `setup`. Runs daily at 06:00:

```
0 6 * * * myapps . /opt/myapps/.env && /opt/myapps/myapps cron >> /opt/myapps/logs/cron.log 2>&1
```

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
 [release]       ◄── auto-bump version, cross-compile aarch64, create GitHub Release
    │
    ▼
 [deploy-stage]  ◄── download release binary, upload to server, install + restart
    │ smoke test /login → 200
    ▼
 [deploy-prod]   ◄── download same release binary, upload to server, install + restart
    │ smoke test /login → 200
    ▼
  Done
```

### Versioning

Version is auto-bumped on each merge to `main` based on commit message prefixes:

| Prefix        | Bump  | Example                          |
|---------------|-------|----------------------------------|
| `[FEAT-*]`   | minor | `[FEAT-42] Add new dashboard`    |
| `[BREAKING-*]`| major | `[BREAKING] Remove legacy API`  |
| anything else | patch | `[BUG-99] Fix login redirect`    |

The workflow can also be triggered manually via `workflow_dispatch` with an
explicit bump type override (patch, minor, or major).

The release commit (`[release] v0.2.0`) is automatically skipped by both CI
and CD to avoid infinite loops.

CI (format, clippy, tests) runs separately via `ci.yml`. The CD pipeline
trusts that CI has already passed on `main`.

### GitHub configuration

The CD workflow requires two GitHub **Environments** (`staging` and
`production`), each with the following configuration:

**Secrets** (repo-level or per-environment):

| Secret             | Description                                         |
|--------------------|-----------------------------------------------------|
| `SSH_PRIVATE_KEY`  | Ed25519 private key authorized on the server        |
| `SSH_KNOWN_HOSTS`  | Output of `ssh-keyscan <server-host>`               |

**Environment variables** (per GitHub Environment):

| Variable                  | Example (staging)           | Example (production)       |
|---------------------------|-----------------------------|----------------------------|
| `DEPLOY_SERVER`           | `deploy@odroid.local`       | `deploy@odroid.local`      |
| `DEPLOY_DOMAIN`           | `stage.yourdomain.com`      | `yourdomain.com`           |
| `DEPLOY_REMOTE_DIR`       | `/opt/myapps-stage`         | `/opt/myapps`              |
| `DEPLOY_REMOTE_BUILD_DIR` | `~/myapps-stage-build`      | `~/myapps-stage-build`     |
| `DEPLOY_SERVICE_NAME`     | `myapps-stage`              | `myapps`                   |
| `DEPLOY_NGINX_SITE`       | `myapps-stage`              | `myapps`                   |
| `DEPLOY_PORT`             | `3001`                      | `3000`                     |
| `DEPLOY_CRON_ENABLED`     | `false`                     | `true`                     |
| `DEPLOY_ICON`             | `icon-stage.svg`            | `icon.svg`                 |
| `DEPLOY_SEED`             | `true`                      | `false`                    |

These match the values in `deploy/*.env.example`.

### Server prerequisites for CI/CD

The same `deploy` user is used for both manual and CI/CD deploys. The
`DEPLOY_CI=true` flag tells `deploy.sh` to skip `-t` (TTY allocation) since
CI runners have no interactive terminal.

### Manual trigger

The CD workflow supports `workflow_dispatch`, so you can trigger a deploy
manually from the GitHub Actions UI without pushing a commit.

## nginx + HTTPS

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
ssh odroid-deploy 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d stage.yourdomain.com'

# 5. Deploy
./deploy.sh stage deploy

# 6. Create a user (invite link or direct)
ssh odroid-deploy 'sudo -u myapps /opt/myapps-stage/myapps invite'
# Or: ssh odroid-deploy 'sudo -u myapps /opt/myapps-stage/myapps create-user --username yourname --password yourpass'
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
├── logs/
└── static/

~/myapps-stage-build/      # Build directory (owned by deploy user)
```
