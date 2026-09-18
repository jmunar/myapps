#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── Usage ──────────────────────────────────────────────────────────
usage() {
    ENVS=$(ls "$SCRIPT_DIR"/deploy/*.env 2>/dev/null | xargs -I{} basename {} .env | tr '\n' ' ')
    cat <<EOF
Usage: $0 <env> <command>

Environments: ${ENVS:-none found}

Commands:
  release-deploy <dir>     Upload pre-built binary + static from extracted tarball, install + restart
  build                    Build the release binary on the server
  deploy                   Sync source + build on server + install + restart
  install                  Sync source + install + restart (skip build; set DEPLOY_BINARY_DIR to use a pre-built binary)
  setup                    First-time server setup (user, dirs, systemd, cron)
  restart                  Restart the service on the server
  logs                     Tail the server logs
  status                   Show service status

Set DEPLOY_SERVER in the environment to override the config file's value —
needed for 'setup', which requires more sudo than the deploy user is granted:
  DEPLOY_SERVER=you@host $0 <env> setup
EOF
    exit 1
}

[[ $# -ge 2 ]] || usage

# ── Load environment config ───────────────────────────────────────
ENV_NAME="$1"
COMMAND="$2"
ENV_FILE="$SCRIPT_DIR/deploy/${ENV_NAME}.env"

if [[ ! -f "$ENV_FILE" ]]; then
    echo "Error: config file not found: $ENV_FILE"
    echo "Available environments:"
    ls "$SCRIPT_DIR"/deploy/*.env 2>/dev/null | xargs -I{} basename {} .env | sed 's/^/  /'
    exit 1
fi

# An explicit DEPLOY_SERVER in the caller's environment wins over the config
# file, which would otherwise clobber it when sourced. `setup` needs full sudo,
# so it is normally run against an admin account rather than the restricted
# deploy user: DEPLOY_SERVER=you@host ./deploy.sh prod setup
SERVER_OVERRIDE="${DEPLOY_SERVER:-}"

# shellcheck source=/dev/null
source "$ENV_FILE"

SERVER="${SERVER_OVERRIDE:-$DEPLOY_SERVER}"
SSH_PORT="${DEPLOY_SSH_PORT:-22}"
EXTRA_ARG="${3:-}"

# Multiplex SSH: one TCP connection shared across the many rapid ssh/scp/rsync
# invocations a deploy makes. Avoids tripping sshd MaxStartups / fail2ban and
# speeds things up (no repeated handshakes). Skipped in CI where each step runs
# in a fresh runner with its own single connection.
SSH_MUX_OPTS=""
if [[ "${DEPLOY_CI:-false}" != "true" ]]; then
    SSH_MUX_OPTS="-o ControlMaster=auto -o ControlPath=/tmp/myapps-ssh-%C -o ControlPersist=60s"
fi

# ── Helpers ────────────────────────────────────────────────────────

# In CI (DEPLOY_CI=true) we skip -t (no TTY available for sudo prompts).
# The deploy SSH user must have passwordless sudo configured on the server.
ssh_server() {
    if [[ "${DEPLOY_CI:-false}" == "true" ]]; then
        ssh $SSH_MUX_OPTS -p "$SSH_PORT" "$SERVER" "$@"
    else
        ssh $SSH_MUX_OPTS -t -p "$SSH_PORT" "$SERVER" "$@"
    fi
}

sync_source() {
    echo "▸ Syncing source to $SERVER:$DEPLOY_REMOTE_BUILD_DIR..."
    rsync -az --delete \
        -e "ssh $SSH_MUX_OPTS -p $SSH_PORT" \
        --exclude target \
        --exclude .git \
        --exclude data \
        --exclude models \
        --exclude '.env' \
        ./ "$SERVER:$DEPLOY_REMOTE_BUILD_DIR/"
}

release_install() {
    local release_dir="${1:?Usage: $0 <env> release-deploy <release-dir>}"
    [[ -f "$release_dir/myapps" ]] || { echo "Error: binary not found: $release_dir/myapps"; exit 1; }
    [[ -d "$release_dir/static" ]] || { echo "Error: static dir not found: $release_dir/static"; exit 1; }
    echo "▸ Uploading release binary to $SERVER..."
    scp $SSH_MUX_OPTS -P "$SSH_PORT" "$release_dir/myapps" "$SERVER:/tmp/myapps.new"
    echo "▸ Installing binary..."
    ssh_server DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" bash <<'INSTALL'
set -euo pipefail
sudo mv /tmp/myapps.new $DEPLOY_REMOTE_DIR/myapps.new
sudo mv $DEPLOY_REMOTE_DIR/myapps.new $DEPLOY_REMOTE_DIR/myapps
sudo chown myapps:myapps $DEPLOY_REMOTE_DIR/myapps
sudo chmod +x $DEPLOY_REMOTE_DIR/myapps
INSTALL
    echo "▸ Syncing static files..."
    rsync -az --delete -e "ssh $SSH_MUX_OPTS -p $SSH_PORT" "$release_dir/static/" "$SERVER:/tmp/myapps-static/"
    ssh_server DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" DEPLOY_ICON="${DEPLOY_ICON:-icon.svg}" bash <<'STATIC'
set -euo pipefail
sudo rsync -a --delete /tmp/myapps-static/ $DEPLOY_REMOTE_DIR/static/
if [[ -n "$DEPLOY_ICON" && "$DEPLOY_ICON" != "icon.svg" ]]; then
    sudo cp $DEPLOY_REMOTE_DIR/static/$DEPLOY_ICON $DEPLOY_REMOTE_DIR/static/icon.svg
fi
sudo chown -R myapps:myapps $DEPLOY_REMOTE_DIR/static
rm -rf /tmp/myapps-static
STATIC
}

build() {
    sync_source
    echo "▸ Building release on $SERVER..."
    ssh $SSH_MUX_OPTS -p "$SSH_PORT" "$SERVER" "source \$HOME/.cargo/env && export RUSTC_WRAPPER=sccache && cd $DEPLOY_REMOTE_BUILD_DIR && cargo build --release"
}

install() {
    local binary_dir="${DEPLOY_BINARY_DIR:-$DEPLOY_REMOTE_BUILD_DIR}"
    echo "▸ Installing binary and static files..."
    ssh_server DEPLOY_BINARY_DIR="$binary_dir" DEPLOY_REMOTE_BUILD_DIR="$DEPLOY_REMOTE_BUILD_DIR" DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" DEPLOY_ICON="${DEPLOY_ICON:-icon.svg}" bash <<'INSTALL'
set -euo pipefail
sudo cp $DEPLOY_BINARY_DIR/target/release/myapps $DEPLOY_REMOTE_DIR/myapps.new
sudo mv $DEPLOY_REMOTE_DIR/myapps.new $DEPLOY_REMOTE_DIR/myapps
sudo chown myapps:myapps $DEPLOY_REMOTE_DIR/myapps
sudo chmod +x $DEPLOY_REMOTE_DIR/myapps
sudo rsync -a --delete $DEPLOY_REMOTE_BUILD_DIR/static/ $DEPLOY_REMOTE_DIR/static/
# Copy environment-specific icon as icon.svg
if [[ -n "$DEPLOY_ICON" && "$DEPLOY_ICON" != "icon.svg" ]]; then
    sudo cp $DEPLOY_REMOTE_DIR/static/$DEPLOY_ICON $DEPLOY_REMOTE_DIR/static/icon.svg
fi
sudo chown -R myapps:myapps $DEPLOY_REMOTE_DIR/static
INSTALL
}

restart() {
    echo "▸ Restarting $DEPLOY_SERVICE_NAME service..."
    ssh_server "sudo systemctl restart $DEPLOY_SERVICE_NAME"
    echo "▸ Done. Checking status..."
    ssh_server "sudo systemctl --no-pager status $DEPLOY_SERVICE_NAME"
}

# Write the systemd unit and reload systemd.
#
# Runs on every deploy, not just setup: the unit is part of the deployment, and
# when it only shipped during setup a change here would sit in the repo while
# the server kept running the old sandbox — which is exactly how an upload
# directory ends up outside ReadWritePaths and fails with EROFS.
#
# Operator drop-ins (systemctl edit -> <service>.service.d/*.conf) are separate
# files and survive this.
write_unit() {
    echo "▸ Writing systemd unit for $DEPLOY_SERVICE_NAME..."
    ssh_server \
        DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" \
        DEPLOY_SERVICE_NAME="$DEPLOY_SERVICE_NAME" \
        DEPLOY_FILE_CLIPBOARD_DIR="${DEPLOY_FILE_CLIPBOARD_DIR:-}" \
        bash <<'UNIT'
set -euo pipefail

# The app reads FILE_CLIPBOARD_DIR from .env, so the sandbox has to follow that
# same value — not a copy of it in the deploy config, which drifts. Fall back to
# the deploy variable, then to the default location.
# \042 and \047 are " and ' — stripping any quotes around the value without
# dragging shell quoting through two levels of heredoc.
FC_DIR="$(sudo sed -n 's/^FILE_CLIPBOARD_DIR=//p' "$DEPLOY_REMOTE_DIR/.env" 2>/dev/null | tail -1 | tr -d '\042\047')"
FC_DIR="${FC_DIR:-${DEPLOY_FILE_CLIPBOARD_DIR:-$DEPLOY_REMOTE_DIR/data/file_clipboard}}"

sudo mkdir -p "$FC_DIR"
sudo chown -R myapps:myapps "$FC_DIR"

# `ProtectSystem=strict` mounts the whole filesystem read-only for this service,
# so every directory it writes to must appear in ReadWritePaths — ownership is
# not enough, and a missing entry surfaces as EROFS ("Read-only file system"),
# never as a permission error. `RequiresMountsFor` stops the service starting
# before an external disk is mounted; without it a boot-time race puts uploads
# in the directory *underneath* the mountpoint, where they disappear from view
# the moment the disk mounts over them.
if [[ "$FC_DIR" == "$DEPLOY_REMOTE_DIR"/* ]]; then
    UNIT_RW_PATHS="$DEPLOY_REMOTE_DIR"
    UNIT_REQUIRES_MOUNTS=""
else
    UNIT_RW_PATHS="$DEPLOY_REMOTE_DIR $FC_DIR"
    UNIT_REQUIRES_MOUNTS="RequiresMountsFor=$FC_DIR"
fi
echo "  Storage directory: $FC_DIR"
echo "  ReadWritePaths:    $UNIT_RW_PATHS"

sudo tee /etc/systemd/system/$DEPLOY_SERVICE_NAME.service > /dev/null <<SERVICE
[Unit]
Description=MyApps platform ($DEPLOY_SERVICE_NAME)
After=network.target
$UNIT_REQUIRES_MOUNTS

[Service]
Type=simple
User=myapps
Group=myapps
WorkingDirectory=$DEPLOY_REMOTE_DIR
ExecStart=$DEPLOY_REMOTE_DIR/myapps serve
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$UNIT_RW_PATHS

[Install]
WantedBy=multi-user.target
SERVICE

sudo systemctl daemon-reload
echo "  Installed $DEPLOY_SERVICE_NAME.service"
UNIT
}

setup() {
    echo "▸ Running first-time server setup on $SERVER ($ENV_NAME)..."
    echo "  (you may be prompted for your sudo password)"
    ssh_server \
        DEPLOY_DOMAIN="$DEPLOY_DOMAIN" \
        DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" \
        DEPLOY_SERVICE_NAME="$DEPLOY_SERVICE_NAME" \
        DEPLOY_NGINX_SITE="$DEPLOY_NGINX_SITE" \
        DEPLOY_PORT="$DEPLOY_PORT" \
        DEPLOY_CRON_ENABLED="$DEPLOY_CRON_ENABLED" \
        DEPLOY_FILE_CLIPBOARD_DIR="${DEPLOY_FILE_CLIPBOARD_DIR:-}" \
        DEPLOY_APPS="${DEPLOY_APPS:-}" \
        DEPLOY_SEED="${DEPLOY_SEED:-false}" \
        DEPLOY_AUTH_SSO_HEADER="${DEPLOY_AUTH_SSO_HEADER:-}" \
        DEPLOY_EXTERNAL_APPS="${DEPLOY_EXTERNAL_APPS:-}" \
        ENV_NAME="$ENV_NAME" \
        bash <<'SETUP'
set -euo pipefail

# Install Rust if not present
if ! command -v cargo &>/dev/null; then
    echo "  Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo "  Rust installed"
fi

# Install build dependencies
echo "  Installing build dependencies..."
sudo apt-get update -qq
sudo apt-get install -y pkg-config libssl-dev sqlite3

# Install sccache if not present
if ! command -v sccache &>/dev/null; then
    echo "  Installing sccache..."
    cargo install sccache --locked
    echo "  sccache installed"
fi

# Create dedicated system user (no login shell, no home dir)
if ! id myapps &>/dev/null; then
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin myapps
    echo "  Created user 'myapps'"
fi

# Create directory structure
sudo mkdir -p $DEPLOY_REMOTE_DIR/{data,static}

# FileClipboard storage. When it sits outside the deploy directory (an external
# disk, typically) the systemd sandbox and the mount ordering below both have to
# account for it.
FC_DIR="${DEPLOY_FILE_CLIPBOARD_DIR:-$DEPLOY_REMOTE_DIR/data/file_clipboard}"
sudo mkdir -p "$FC_DIR"
sudo chown -R myapps:myapps "$FC_DIR"
sudo chown -R myapps:myapps $DEPLOY_REMOTE_DIR
sudo chmod 750 $DEPLOY_REMOTE_DIR

# Create .env template if it doesn't exist
if [[ ! -f $DEPLOY_REMOTE_DIR/.env ]]; then
    sudo tee $DEPLOY_REMOTE_DIR/.env > /dev/null <<ENV
DATABASE_URL=sqlite://$DEPLOY_REMOTE_DIR/data/myapps.db
BASE_URL=https://$DEPLOY_DOMAIN
ENCRYPTION_KEY=
VAPID_PRIVATE_KEY=
VAPID_PUBLIC_KEY=
VAPID_SUBJECT=mailto:you@example.com
WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli
WHISPER_MODELS_DIR=/opt/whisper.cpp/models
LLAMA_SERVER_URL=
FILE_CLIPBOARD_DIR=${DEPLOY_FILE_CLIPBOARD_DIR:-$DEPLOY_REMOTE_DIR/data/file_clipboard}
FILE_CLIPBOARD_RETENTION_DAYS=7
FILE_CLIPBOARD_MAX_FILE_BYTES=5368709120
FILE_CLIPBOARD_USER_QUOTA_BYTES=21474836480
FILE_CLIPBOARD_MIN_FREE_BYTES=2147483648
BIND_ADDR=127.0.0.1:$DEPLOY_PORT
DEPLOY_APPS=${DEPLOY_APPS:-}
AUTH_SSO_HEADER=${DEPLOY_AUTH_SSO_HEADER:-}
EXTERNAL_APPS=${DEPLOY_EXTERNAL_APPS:-}
SEED=${DEPLOY_SEED:-false}
CLEANUP_INACTIVE_DAYS=0
ENV
    sudo chown myapps:myapps $DEPLOY_REMOTE_DIR/.env
    sudo chmod 600 $DEPLOY_REMOTE_DIR/.env
    echo "  Created $DEPLOY_REMOTE_DIR/.env — edit it with your values"
fi

# The systemd unit is written by write_unit() from the deploy script, which runs
# on every deploy as well as on setup.

# Install cron job only if enabled
if [[ "$DEPLOY_CRON_ENABLED" == "true" ]]; then
    sudo tee /etc/cron.d/$DEPLOY_SERVICE_NAME > /dev/null <<CRON
# MyApps daily scheduled tasks ($DEPLOY_SERVICE_NAME)
0 6 * * * myapps $DEPLOY_REMOTE_DIR/myapps cron
CRON
    sudo chmod 644 /etc/cron.d/$DEPLOY_SERVICE_NAME
    echo "  Installed cron job (daily at 06:00)"
else
    echo "  Cron disabled for this environment"
fi

# Install nginx site config if not present
if [[ ! -f /etc/nginx/sites-available/$DEPLOY_NGINX_SITE ]]; then
    sudo tee /etc/nginx/sites-available/$DEPLOY_NGINX_SITE > /dev/null <<NGINX
server {
    listen 80;
    server_name $DEPLOY_DOMAIN;

    # FileClipboard uploads. Must be at least FILE_CLIPBOARD_MAX_FILE_BYTES;
    # nginx's 1m default rejects anything larger before the app sees it.
    client_max_body_size 5g;
    # Stream request bodies straight through. Buffering would spool a whole
    # 5 GB upload to nginx's temp directory first — twice the disk writes, and
    # no progress reaches the app until the transfer is already finished.
    proxy_request_buffering off;

    location / {
        proxy_pass http://127.0.0.1:$DEPLOY_PORT/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
NGINX
    sudo ln -sf /etc/nginx/sites-available/$DEPLOY_NGINX_SITE /etc/nginx/sites-enabled/$DEPLOY_NGINX_SITE
    sudo nginx -t && sudo systemctl reload nginx
    echo "  Installed nginx config for $DEPLOY_DOMAIN (HTTP only)"
    echo "  To enable HTTPS, run: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DEPLOY_DOMAIN"
else
    # The site config is written once and never rewritten, so a server set up
    # before FileClipboard existed keeps nginx's 1 MB default and rejects every
    # upload before the app sees it. Say so instead of silently skipping.
    echo "  nginx config already exists, not overwriting"
    MISSING=""
    grep -q 'client_max_body_size' /etc/nginx/sites-available/$DEPLOY_NGINX_SITE || MISSING="$MISSING client_max_body_size"
    grep -q 'proxy_request_buffering' /etc/nginx/sites-available/$DEPLOY_NGINX_SITE || MISSING="$MISSING proxy_request_buffering"
    if [[ -n "$MISSING" ]]; then
        echo "  WARNING: missing directive(s) in /etc/nginx/sites-available/$DEPLOY_NGINX_SITE:$MISSING"
        echo "           FileClipboard uploads will fail until you add:"
        echo "             client_max_body_size 5g;"
        echo "             proxy_request_buffering off;"
    fi
fi

echo ""
echo "Setup complete. Next steps:"
echo "  1. Edit $DEPLOY_REMOTE_DIR/.env with your values"
echo "  2. From your dev machine, run: ./deploy.sh $ENV_NAME deploy"
echo "  3. Create a user: sudo -u myapps $DEPLOY_REMOTE_DIR/myapps create-user --username <name> --password <pass>"
echo "  4. Set up HTTPS: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DEPLOY_DOMAIN"
SETUP

    write_unit
}

# ── Command dispatch ───────────────────────────────────────────────
case "${COMMAND}" in
    release-deploy) release_install "$EXTRA_ARG" && write_unit && restart ;;
    build)   build ;;
    deploy)  build && install && write_unit && restart ;;
    install) sync_source && install && write_unit && restart ;;
    setup)   setup ;;
    restart) restart ;;
    logs)    ssh_server "sudo journalctl -u $DEPLOY_SERVICE_NAME -f --no-pager" ;;
    status)  ssh_server "sudo systemctl --no-pager status $DEPLOY_SERVICE_NAME" ;;
    *)       usage ;;
esac
