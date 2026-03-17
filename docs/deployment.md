# MyApps — Deployment

## Target Environment

- **Hardware**: Odroid N2 (4 GB RAM, ARM64)
- **OS**: Ubuntu Server 24.04 (aarch64)
- **Reverse proxy**: nginx + certbot (HTTPS)
- **Init system**: systemd
- **URL**: `https://yourdomain.com/myapps`

## Build Strategy

The project is built **natively on the Odroid** rather than cross-compiled
on the development machine. The deploy script rsyncs the source code to the
server, compiles there, and installs the binary. The Odroid N2 with 4 GB RAM
handles Rust compilation without issues.

This avoids the complexity of cross-compilation toolchains (linkers, Docker,
`cross`) when developing on macOS.

## Prerequisites

### Development machine (macOS)

- SSH access to the Odroid (key-based auth recommended)
- `rsync` (pre-installed on macOS)

### Server (Odroid N2)

- SSH access configured
- nginx installed and running
- Your SSH user must have `sudo` privileges
- Rust toolchain — installed automatically by `./deploy.sh prod setup`, or manually:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- whisper.cpp + ffmpeg (for VoiceToText) — see [whisper.cpp section](#whispercpp-voicetotext) below

#### Passwordless sudo (recommended)

The deploy script uses `sudo` over SSH for setup, restart, and log commands.
To avoid being prompted for a password each time, configure passwordless sudo
for your user on the Odroid:

```bash
# On the Odroid
sudo visudo -f /etc/sudoers.d/youruser
```

Add:

```
youruser ALL=(ALL) NOPASSWD: ALL
```

If you prefer not to do this, the script will prompt for your password
interactively (via `ssh -t`).

## Quick Start

```bash
# 1. Configure server address
export MYAPPS_SERVER="user@odroid.local"

# 2. First-time server setup (installs Rust, creates user, dirs, systemd, cron, nginx)
./deploy.sh prod setup

# 3. SSH into the server and edit /opt/myapps/.env with your values

# 4. Set up HTTPS on the server
ssh $MYAPPS_SERVER 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d yourdomain.com'

# 5. Sync source, build on server, install, and start the service
./deploy.sh prod deploy

# 6. Create your first user
ssh $MYAPPS_SERVER 'sudo -u myapps /opt/myapps/myapps create-user --username yourname --password yourpass'
```

## deploy.sh Commands

Usage: `./deploy.sh <env> <command>`

| Command   | Description                                         |
|-----------|-----------------------------------------------------|
| `setup`   | First-time server provisioning (+ Rust install)     |
| `deploy`  | Rsync source + build on server + install + restart  |
| `build`   | Rsync source + build on server (no install)         |
| `restart` | Restart the service                                 |
| `logs`    | Tail the service logs (journalctl)                  |
| `status`  | Show service status                                 |

Available environments are defined by config files in `deploy/`:

| Environment | Config file      | URL                                      | Port |
|-------------|------------------|------------------------------------------|------|
| `prod`      | `deploy/prod.env` | `https://yourdomain.com/myapps`          | 3000 |
| `stage`     | `deploy/stage.env` | `https://stage.munarriz.mooo.com/myapps` | 3001 |

Configure the SSH target via environment variable:

```bash
export MYAPPS_SERVER="user@odroid.local"   # SSH target (used by both envs)
```

## Deploy Flow

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
4. Creates `/opt/myapps/{data,logs,static}` with proper ownership
5. Creates `/opt/myapps/.env` template (chmod 600)
6. Installs the `myapps.service` systemd unit
7. Installs a cron job for daily sync at 06:00
8. Installs an nginx site config for the configured domain (HTTP, proxying
   `/myapps/` to `127.0.0.1:3000`)

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
│   └── sync.log           # Cron job output
└── static/                # (reserved for future use)

~/myapps-build/            # Build directory (owned by your SSH user)
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
BASE_URL=https://yourdomain.com/myapps   # Public URL (path becomes BASE_PATH)
ENCRYPTION_KEY=                            # 32-byte hex (openssl rand -hex 32)
VAPID_PRIVATE_KEY=                         # base64url-encoded EC private key
VAPID_PUBLIC_KEY=                          # base64url-encoded uncompressed public key
VAPID_SUBJECT=mailto:you@example.com       # VAPID subject claim
WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli   # whisper.cpp binary
WHISPER_MODELS_DIR=/opt/whisper.cpp/models                # GGML model directory
BIND_ADDR=127.0.0.1:3000
```

Only `DATABASE_URL` and `BIND_ADDR` are required to start the server.
`BASE_URL` is needed when served behind a reverse proxy subpath (the path
component is used as the URL prefix). `ENCRYPTION_KEY` is needed for
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
0 6 * * * myapps . /opt/myapps/.env && /opt/myapps/myapps sync >> /opt/myapps/logs/sync.log 2>&1
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

## nginx + HTTPS

The `setup` command installs an HTTP-only nginx config at
`/etc/nginx/sites-available/myapps` with `server_name` set to your domain.
The config proxies `/myapps/` to `127.0.0.1:3000/` (stripping the prefix).

To enable HTTPS:

```bash
sudo apt install python3-certbot-nginx
sudo certbot --nginx -d yourdomain.com
```

Certbot will modify the nginx config to add `listen 443 ssl` with the
certificate paths and redirect HTTP to HTTPS automatically.

The app is then accessible at `https://yourdomain.com/myapps`.

## Staging Environment

A staging instance runs alongside production on the same Odroid, at
`https://stage.munarriz.mooo.com/myapps`. It uses a separate database, systemd
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

# 3. DNS: add stage.munarriz.mooo.com at freedns.afraid.org

# 4. HTTPS
ssh $MYAPPS_SERVER 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d stage.munarriz.mooo.com'

# 5. Deploy
./deploy.sh stage deploy

# 6. Create a user
ssh $MYAPPS_SERVER 'sudo -u myapps /opt/myapps-stage/myapps create-user --username yourname --password yourpass'
```

### Deploying with seed data

When `SEED_REBUILD=true` is set, the deploy command wipes and re-seeds all apps
listed in `DEPLOY_SEED_APPS` after restarting the service:

```bash
SEED_REBUILD=true ./deploy.sh stage deploy
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

~/myapps-stage-build/      # Build directory (owned by SSH user)
```
