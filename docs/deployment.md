# LeanFin — Deployment

## Target Environment

- **Hardware**: Odroid N2 (4 GB RAM, ARM64)
- **OS**: Ubuntu Server 24.04 (aarch64)
- **Reverse proxy**: nginx + certbot (HTTPS)
- **Init system**: systemd
- **URL**: `https://yourdomain.com/leanfin`

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
export LEANFIN_SERVER="user@odroid.local"
export LEANFIN_DOMAIN="yourdomain.com"

# 2. First-time server setup (installs Rust, creates user, dirs, systemd, cron, nginx)
./deploy.sh setup

# 3. SSH into the server and edit /opt/leanfin/.env with your values

# 4. Set up HTTPS on the server
ssh $LEANFIN_SERVER 'sudo apt install python3-certbot-nginx && sudo certbot --nginx -d yourdomain.com'

# 5. Sync source, build on server, install, and start the service
./deploy.sh deploy

# 6. Create your first user
ssh $LEANFIN_SERVER 'sudo -u leanfin /opt/leanfin/leanfin create-user --username yourname --password yourpass'
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
export LEANFIN_SERVER="user@odroid.local"   # SSH target
export LEANFIN_DOMAIN="yourdomain.com"      # Domain for nginx server_name
```

## Deploy Flow

```
Dev machine                         Odroid N2
───────────                         ─────────
./deploy.sh deploy
  │
  ├─ rsync source ──────────────▸  ~/leanfin-build/
  │                                  │
  │  ssh: cargo build --release      ├─ compile natively
  │                                  │
  │  ssh: sudo cp binary             ├─ /opt/leanfin/leanfin
  │                                  │
  │  ssh: sudo systemctl restart     └─ service running
  │
  └─ done
```

## What `setup` Does

Run once on a fresh server. It:

1. Installs the Rust toolchain (if not already present)
2. Creates a `leanfin` system user (no login shell)
3. Creates `/opt/leanfin/{data,logs,static}` with proper ownership
4. Creates `/opt/leanfin/.env` template (chmod 600)
5. Installs the `leanfin.service` systemd unit
6. Installs a cron job for daily sync at 06:00
7. Installs an nginx site config for the configured domain (HTTP, proxying
   `/leanfin/` to `127.0.0.1:3000`)

After setup, enable HTTPS with certbot (see Quick Start step 4).

## Directory Structure on Server

```
/opt/leanfin/              # Runtime (owned by leanfin user)
├── leanfin                # Binary
├── .env                   # Environment variables (chmod 600)
├── private.pem            # Enable Banking RSA private key (chmod 600)
├── data/
│   └── leanfin.db         # SQLite database (created on first run)
├── logs/
│   └── sync.log           # Cron job output
└── static/                # (reserved for future use)

~/leanfin-build/           # Build directory (owned by your SSH user)
├── src/
├── Cargo.toml
├── Cargo.lock
└── target/                # Compilation artifacts (cached between deploys)
```

The build directory (`~/leanfin-build`) is separate from the runtime directory
(`/opt/leanfin`). Cargo's `target/` is cached on the server, so subsequent
builds are incremental and fast.

## Environment Variables

File: `/opt/leanfin/.env`

```bash
DATABASE_URL=sqlite:///opt/leanfin/data/leanfin.db
BASE_URL=https://yourdomain.com/leanfin   # Public URL (path becomes BASE_PATH)
ENABLE_BANKING_APP_ID=              # UUID from Enable Banking control panel
ENABLE_BANKING_KEY_PATH=/opt/leanfin/private.pem   # RSA private key
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
BIND_ADDR=127.0.0.1:3000
```

Only `DATABASE_URL` and `BIND_ADDR` are required to start the server.
`BASE_URL` is needed when served behind a reverse proxy subpath (the path
component is used as the URL prefix). The Enable Banking variables are needed
to link bank accounts. Copy
your Enable Banking private key to `/opt/leanfin/private.pem` (chmod 600,
owned by leanfin).

## systemd Service

Installed at `/etc/systemd/system/leanfin.service` by `setup`.

```bash
sudo systemctl enable leanfin    # auto-start on boot
sudo systemctl start leanfin
sudo systemctl status leanfin
sudo journalctl -u leanfin -f    # tail logs
```

## Cron Job

Installed at `/etc/cron.d/leanfin` by `setup`. Runs daily at 06:00:

```
0 6 * * * leanfin . /opt/leanfin/.env && /opt/leanfin/leanfin sync >> /opt/leanfin/logs/sync.log 2>&1
```

## nginx + HTTPS

The `setup` command installs an HTTP-only nginx config at
`/etc/nginx/sites-available/leanfin` with `server_name` set to your domain.
The config proxies `/leanfin/` to `127.0.0.1:3000/` (stripping the prefix).

To enable HTTPS:

```bash
sudo apt install python3-certbot-nginx
sudo certbot --nginx -d yourdomain.com
```

Certbot will modify the nginx config to add `listen 443 ssl` with the
certificate paths and redirect HTTP to HTTPS automatically.

The app is then accessible at `https://yourdomain.com/leanfin`.
