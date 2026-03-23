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
  build         Build the release binary on the server
  deploy        Sync source + build on server + install + restart
  install       Sync source + install + restart (skip build; set DEPLOY_BINARY_DIR to use a pre-built binary)
  setup         First-time server setup (user, dirs, systemd, cron)
  restart       Restart the service on the server
  logs          Tail the server logs
  status        Show service status
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

# shellcheck source=/dev/null
source "$ENV_FILE"

SERVER="$DEPLOY_SERVER"

# ── Helpers ────────────────────────────────────────────────────────

# In CI (DEPLOY_CI=true) we skip -t (no TTY available for sudo prompts).
# The deploy SSH user must have passwordless sudo configured on the server.
ssh_server() {
    if [[ "${DEPLOY_CI:-false}" == "true" ]]; then
        ssh "$SERVER" "$@"
    else
        ssh -t "$SERVER" "$@"
    fi
}

sync_source() {
    echo "▸ Syncing source to $SERVER:$DEPLOY_REMOTE_BUILD_DIR..."
    rsync -az --delete \
        --exclude target \
        --exclude .git \
        --exclude data \
        --exclude '.env' \
        ./ "$SERVER:$DEPLOY_REMOTE_BUILD_DIR/"
}

build() {
    sync_source
    echo "▸ Building release on $SERVER..."
    ssh "$SERVER" "source \$HOME/.cargo/env && cd $DEPLOY_REMOTE_BUILD_DIR && cargo build --release"
}

install() {
    local binary_dir="${DEPLOY_BINARY_DIR:-$DEPLOY_REMOTE_BUILD_DIR}"
    echo "▸ Installing binary and static files..."
    ssh_server DEPLOY_BINARY_DIR="$binary_dir" DEPLOY_REMOTE_BUILD_DIR="$DEPLOY_REMOTE_BUILD_DIR" DEPLOY_REMOTE_DIR="$DEPLOY_REMOTE_DIR" DEPLOY_ICON="$DEPLOY_ICON" bash <<'INSTALL'
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

cleanup() {
    if [[ "${DEPLOY_USER_CLEANUP_DAYS:-0}" == "0" ]]; then
        return
    fi
    echo "▸ Cleaning up inactive users (>${DEPLOY_USER_CLEANUP_DAYS} days)..."
    ssh_server "sudo -u myapps $DEPLOY_REMOTE_DIR/myapps cleanup-users --days $DEPLOY_USER_CLEANUP_DAYS"
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
sudo mkdir -p $DEPLOY_REMOTE_DIR/{data,logs,static}
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
BIND_ADDR=127.0.0.1:$DEPLOY_PORT
DEPLOY_APPS=${DEPLOY_APPS:-}
SEED=${DEPLOY_SEED:-false}
CLEANUP_INACTIVE_DAYS=${DEPLOY_USER_CLEANUP_DAYS:-0}
ENV
    sudo chown myapps:myapps $DEPLOY_REMOTE_DIR/.env
    sudo chmod 600 $DEPLOY_REMOTE_DIR/.env
    echo "  Created $DEPLOY_REMOTE_DIR/.env — edit it with your values"
fi

# Install systemd service
sudo tee /etc/systemd/system/$DEPLOY_SERVICE_NAME.service > /dev/null <<SERVICE
[Unit]
Description=MyApps platform ($DEPLOY_SERVICE_NAME)
After=network.target

[Service]
Type=simple
User=myapps
Group=myapps
WorkingDirectory=$DEPLOY_REMOTE_DIR
ExecStart=$DEPLOY_REMOTE_DIR/myapps serve
EnvironmentFile=$DEPLOY_REMOTE_DIR/.env
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$DEPLOY_REMOTE_DIR

[Install]
WantedBy=multi-user.target
SERVICE

sudo systemctl daemon-reload
echo "  Installed $DEPLOY_SERVICE_NAME.service"

# Install cron job only if enabled
if [[ "$DEPLOY_CRON_ENABLED" == "true" ]]; then
    sudo tee /etc/cron.d/$DEPLOY_SERVICE_NAME > /dev/null <<CRON
# MyApps daily scheduled tasks ($DEPLOY_SERVICE_NAME)
0 6 * * * myapps . $DEPLOY_REMOTE_DIR/.env && $DEPLOY_REMOTE_DIR/myapps cron >> $DEPLOY_REMOTE_DIR/logs/cron.log 2>&1
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

    location / {
        proxy_pass http://127.0.0.1:$DEPLOY_PORT/;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_read_timeout 120s;
    }
}
NGINX
    sudo ln -sf /etc/nginx/sites-available/$DEPLOY_NGINX_SITE /etc/nginx/sites-enabled/$DEPLOY_NGINX_SITE
    sudo nginx -t && sudo systemctl reload nginx
    echo "  Installed nginx config for $DEPLOY_DOMAIN (HTTP only)"
    echo "  To enable HTTPS, run: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DEPLOY_DOMAIN"
else
    echo "  nginx config already exists, skipping"
fi

echo ""
echo "Setup complete. Next steps:"
echo "  1. Edit $DEPLOY_REMOTE_DIR/.env with your values"
echo "  2. From your dev machine, run: ./deploy.sh $ENV_NAME deploy"
echo "  3. Create a user: sudo -u myapps $DEPLOY_REMOTE_DIR/myapps create-user --username <name> --password <pass>"
echo "  4. Set up HTTPS: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DEPLOY_DOMAIN"
SETUP
}

# ── Command dispatch ───────────────────────────────────────────────
case "${COMMAND}" in
    build)   build ;;
    deploy)  build && install && restart && cleanup ;;
    install) sync_source && install && restart && cleanup ;;
    setup)   setup ;;
    restart) restart ;;
    logs)    ssh_server "sudo journalctl -u $DEPLOY_SERVICE_NAME -f --no-pager" ;;
    status)  ssh_server "sudo systemctl --no-pager status $DEPLOY_SERVICE_NAME" ;;
    *)       usage ;;
esac
