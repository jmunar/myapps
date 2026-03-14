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
- Rust toolchain — installed automatically by `./deploy.sh setup`, or manually:
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
# 1. Configure server address and domain
export MYAPPS_SERVER="user@odroid.local"
export MYAPPS_DOMAIN="yourdomain.com"

# 2. First-time server setup (installs Rust, creates user, dirs, systemd, cron, nginx)
./deploy.sh setup

# 3. SSH into the server and edit /opt/myapps/.env with your values

# 4. Set up HTTPS on the server
ssh $MYAPPS_SERVER 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d yourdomain.com'

# 5. Sync source, build on server, install, and start the service
./deploy.sh deploy

# 6. Create your first user
ssh $MYAPPS_SERVER 'sudo -u myapps /opt/myapps/myapps create-user --username yourname --password yourpass'
```

## deploy.sh Commands

| Command   | Description                                         |
|-----------|-----------------------------------------------------|
| `setup`   | First-time server provisioning (+ Rust install)     |
| `deploy`  | Rsync source + build on server + install + restart  |
| `build`   | Rsync source + build on server (no install)         |
| `restart` | Restart the service                                 |
| `logs`    | Tail the service logs (journalctl)                  |
| `status`  | Show service status                                 |

Configure via environment variables:

```bash
export MYAPPS_SERVER="user@odroid.local"   # SSH target
export MYAPPS_DOMAIN="yourdomain.com"      # Domain for nginx server_name
```

## Deploy Flow

```
Dev machine                         Odroid N2
───────────                         ─────────
./deploy.sh deploy
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
ENABLE_BANKING_APP_ID=              # UUID from Enable Banking control panel
ENABLE_BANKING_KEY_PATH=/opt/myapps/private.pem   # RSA private key
NTFY_URL=http://127.0.0.1:8090              # ntfy server (local)
NTFY_TOPIC=                                 # ntfy topic name
WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli   # whisper.cpp binary
WHISPER_MODELS_DIR=/opt/whisper.cpp/models                # GGML model directory
BIND_ADDR=127.0.0.1:3000
```

Only `DATABASE_URL` and `BIND_ADDR` are required to start the server.
`BASE_URL` is needed when served behind a reverse proxy subpath (the path
component is used as the URL prefix). The Enable Banking variables are needed
to link bank accounts. Copy
your Enable Banking private key to `/opt/myapps/private.pem` (chmod 600,
owned by myapps).

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

## ntfy (Self-Hosted Notifications)

ntfy runs as a single binary and sends push notifications to your phone.

### Install

```bash
# On the Odroid — add the ntfy apt repository
sudo mkdir -p /etc/apt/keyrings
sudo curl -L -o /etc/apt/keyrings/ntfy.gpg https://archive.ntfy.sh/apt/keyring.gpg
sudo apt install apt-transport-https
echo "deb [arch=arm64 signed-by=/etc/apt/keyrings/ntfy.gpg] https://archive.ntfy.sh/apt stable main" \
    | sudo tee /etc/apt/sources.list.d/ntfy.list
sudo apt update
sudo apt install ntfy
```

### Configure

Edit `/etc/ntfy/server.yml`:

```yaml
base-url: https://ntfy.munarriz.mooo.com
listen-http: 127.0.0.1:8090
behind-proxy: true
```

### Enable and start

```bash
sudo systemctl enable ntfy
sudo systemctl start ntfy
```

### Add nginx proxy

Create `/etc/nginx/sites-available/ntfy`:

```nginx
server {
    listen 80;
    server_name ntfy.munarriz.mooo.com;

    location / {
        proxy_pass http://127.0.0.1:8090;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

Enable it, reload, and add HTTPS:

```bash
sudo ln -s /etc/nginx/sites-available/ntfy /etc/nginx/sites-enabled/
sudo nginx -s reload
sudo certbot --nginx -d ntfy.munarriz.mooo.com
```

### Connect MyApps

Set these in `/opt/myapps/.env`:

```bash
NTFY_URL=http://127.0.0.1:8090
NTFY_TOPIC=myapps
```

MyApps connects to ntfy locally — no need to go through nginx.

### Subscribe on your phone

Install the ntfy app ([Android](https://play.google.com/store/apps/details?id=io.heckel.ntfy),
[iOS](https://apps.apple.com/app/ntfy/id1625396347)), add your server
`https://ntfy.munarriz.mooo.com`, and subscribe to the `myapps` topic.

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
