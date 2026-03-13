#!/usr/bin/env bash
set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────
# Override these via environment variables or edit the defaults below.
SERVER="${MYAPPS_SERVER:-${LEANFIN_SERVER:-user@odroid.local}}"
DOMAIN="${MYAPPS_DOMAIN:-${LEANFIN_DOMAIN:-yourdomain.com}}"
REMOTE_DIR="${MYAPPS_REMOTE_DIR:-/opt/myapps}"
REMOTE_BUILD_DIR="~/myapps-build"

# ── Usage ──────────────────────────────────────────────────────────
usage() {
    cat <<EOF
Usage: $0 <command>

Commands:
  build         Build the release binary on the server
  deploy        Sync source + build on server + install + restart
  setup         First-time server setup (user, dirs, systemd, cron)
  restart       Restart the service on the server
  logs          Tail the server logs
  status        Show service status
EOF
    exit 1
}

[[ $# -ge 1 ]] || usage

# ── Helpers ────────────────────────────────────────────────────────

sync_source() {
    echo "▸ Syncing source to $SERVER:$REMOTE_BUILD_DIR..."
    rsync -az --delete \
        --exclude target \
        --exclude .git \
        --exclude data \
        --exclude '.env' \
        ./ "$SERVER:$REMOTE_BUILD_DIR/"
}

build() {
    sync_source
    echo "▸ Building release on $SERVER..."
    ssh "$SERVER" "source \$HOME/.cargo/env && cd $REMOTE_BUILD_DIR && cargo build --release"
}

install() {
    echo "▸ Installing binary and static files..."
    ssh -t "$SERVER" bash <<INSTALL
set -euo pipefail
sudo cp $REMOTE_BUILD_DIR/target/release/myapps $REMOTE_DIR/myapps.new
sudo mv $REMOTE_DIR/myapps.new $REMOTE_DIR/myapps
sudo chown myapps:myapps $REMOTE_DIR/myapps
sudo chmod +x $REMOTE_DIR/myapps
sudo rsync -a --delete $REMOTE_BUILD_DIR/static/ $REMOTE_DIR/static/
sudo chown -R myapps:myapps $REMOTE_DIR/static
INSTALL
}

restart() {
    echo "▸ Restarting myapps service..."
    ssh -t "$SERVER" "sudo systemctl restart myapps"
    echo "▸ Done. Checking status..."
    ssh -t "$SERVER" "sudo systemctl --no-pager status myapps"
}

setup() {
    echo "▸ Running first-time server setup on $SERVER..."
    echo "  (you may be prompted for your sudo password)"
    ssh -t "$SERVER" DOMAIN="$DOMAIN" bash <<'SETUP'
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
sudo apt-get install -y pkg-config libssl-dev

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
sudo mkdir -p /opt/myapps/{data,logs,static}
sudo chown -R myapps:myapps /opt/myapps
sudo chmod 750 /opt/myapps

# Create .env template if it doesn't exist
if [[ ! -f /opt/myapps/.env ]]; then
    sudo tee /opt/myapps/.env > /dev/null <<'ENV'
DATABASE_URL=sqlite:///opt/myapps/data/myapps.db
BASE_URL=https://YOURDOMAIN/myapps
ENABLE_BANKING_APP_ID=
ENABLE_BANKING_KEY_PATH=/opt/myapps/private.pem
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
BIND_ADDR=127.0.0.1:3000
ENV
    sudo chown myapps:myapps /opt/myapps/.env
    sudo chmod 600 /opt/myapps/.env
    echo "  Created /opt/myapps/.env — edit it with your values"
fi

# Install systemd service
sudo tee /etc/systemd/system/myapps.service > /dev/null <<'SERVICE'
[Unit]
Description=MyApps platform
After=network.target

[Service]
Type=simple
User=myapps
Group=myapps
WorkingDirectory=/opt/myapps
ExecStart=/opt/myapps/myapps serve
EnvironmentFile=/opt/myapps/.env
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/myapps

[Install]
WantedBy=multi-user.target
SERVICE

sudo systemctl daemon-reload
echo "  Installed myapps.service"

# Install cron job for daily sync at 06:00
sudo tee /etc/cron.d/myapps > /dev/null <<'CRON'
# MyApps daily transaction sync
0 6 * * * myapps . /opt/myapps/.env && /opt/myapps/myapps sync >> /opt/myapps/logs/sync.log 2>&1
CRON
sudo chmod 644 /etc/cron.d/myapps
echo "  Installed cron job (daily at 06:00)"

# Install nginx site config if not present
if [[ ! -f /etc/nginx/sites-available/myapps ]]; then
    sudo tee /etc/nginx/sites-available/myapps > /dev/null <<NGINX
server {
    listen 80;
    server_name $DOMAIN;

    location /myapps/ {
        proxy_pass http://127.0.0.1:3000/;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }

    # Redirect /myapps to /myapps/
    location = /myapps {
        return 301 /myapps/;
    }
}
NGINX
    sudo ln -sf /etc/nginx/sites-available/myapps /etc/nginx/sites-enabled/myapps
    sudo rm -f /etc/nginx/sites-enabled/default
    sudo nginx -t && sudo systemctl reload nginx
    echo "  Installed nginx config for $DOMAIN (HTTP only)"
    echo "  To enable HTTPS, run: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DOMAIN"
else
    echo "  nginx config already exists, skipping"
fi

echo ""
echo "Setup complete. Next steps:"
echo "  1. Edit /opt/myapps/.env with your values"
echo "  2. From your dev machine, run: ./deploy.sh deploy"
echo "  3. Create a user: sudo -u myapps /opt/myapps/myapps create-user --username <name> --password <pass>"
echo "  4. Set up HTTPS: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DOMAIN"
SETUP
}

# ── Command dispatch ───────────────────────────────────────────────
case "${1}" in
    build)   build ;;
    deploy)  build && install && restart ;;
    setup)   setup ;;
    restart) restart ;;
    logs)    ssh -t "$SERVER" "sudo journalctl -u myapps -f --no-pager" ;;
    status)  ssh -t "$SERVER" "sudo systemctl --no-pager status myapps" ;;
    *)       usage ;;
esac
