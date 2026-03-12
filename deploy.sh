#!/usr/bin/env bash
set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────
# Override these via environment variables or edit the defaults below.
SERVER="${LEANFIN_SERVER:-user@odroid.local}"
DOMAIN="${LEANFIN_DOMAIN:-yourdomain.com}"
REMOTE_DIR="${LEANFIN_REMOTE_DIR:-/opt/leanfin}"
REMOTE_BUILD_DIR="~/leanfin-build"

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
    echo "▸ Installing binary..."
    ssh -t "$SERVER" bash <<INSTALL
set -euo pipefail
sudo cp $REMOTE_BUILD_DIR/target/release/leanfin $REMOTE_DIR/leanfin.new
sudo mv $REMOTE_DIR/leanfin.new $REMOTE_DIR/leanfin
sudo chown leanfin:leanfin $REMOTE_DIR/leanfin
sudo chmod +x $REMOTE_DIR/leanfin
INSTALL
}

restart() {
    echo "▸ Restarting leanfin service..."
    ssh -t "$SERVER" "sudo systemctl restart leanfin"
    echo "▸ Done. Checking status..."
    ssh -t "$SERVER" "sudo systemctl --no-pager status leanfin"
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

# Create dedicated system user (no login shell, no home dir)
if ! id leanfin &>/dev/null; then
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin leanfin
    echo "  Created user 'leanfin'"
fi

# Create directory structure
sudo mkdir -p /opt/leanfin/{data,logs,static}
sudo chown -R leanfin:leanfin /opt/leanfin
sudo chmod 750 /opt/leanfin

# Create .env template if it doesn't exist
if [[ ! -f /opt/leanfin/.env ]]; then
    sudo tee /opt/leanfin/.env > /dev/null <<'ENV'
DATABASE_URL=sqlite:///opt/leanfin/data/leanfin.db
BASE_URL=https://YOURDOMAIN/leanfin
ENABLE_BANKING_APP_ID=
ENABLE_BANKING_KEY_PATH=/opt/leanfin/private.pem
TELEGRAM_BOT_TOKEN=
TELEGRAM_CHAT_ID=
BIND_ADDR=127.0.0.1:3000
ENV
    sudo chown leanfin:leanfin /opt/leanfin/.env
    sudo chmod 600 /opt/leanfin/.env
    echo "  Created /opt/leanfin/.env — edit it with your values"
fi

# Install systemd service
sudo tee /etc/systemd/system/leanfin.service > /dev/null <<'SERVICE'
[Unit]
Description=LeanFin expense tracker
After=network.target

[Service]
Type=simple
User=leanfin
Group=leanfin
WorkingDirectory=/opt/leanfin
ExecStart=/opt/leanfin/leanfin serve
EnvironmentFile=/opt/leanfin/.env
Restart=on-failure
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/leanfin

[Install]
WantedBy=multi-user.target
SERVICE

sudo systemctl daemon-reload
echo "  Installed leanfin.service"

# Install cron job for daily sync at 06:00
sudo tee /etc/cron.d/leanfin > /dev/null <<'CRON'
# LeanFin daily transaction sync
0 6 * * * leanfin . /opt/leanfin/.env && /opt/leanfin/leanfin sync >> /opt/leanfin/logs/sync.log 2>&1
CRON
sudo chmod 644 /etc/cron.d/leanfin
echo "  Installed cron job (daily at 06:00)"

# Install nginx site config if not present
if [[ ! -f /etc/nginx/sites-available/leanfin ]]; then
    sudo tee /etc/nginx/sites-available/leanfin > /dev/null <<NGINX
server {
    listen 80;
    server_name $DOMAIN;

    location /leanfin/ {
        proxy_pass http://127.0.0.1:3000/;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }

    # Redirect /leanfin to /leanfin/
    location = /leanfin {
        return 301 /leanfin/;
    }
}
NGINX
    sudo ln -sf /etc/nginx/sites-available/leanfin /etc/nginx/sites-enabled/leanfin
    sudo rm -f /etc/nginx/sites-enabled/default
    sudo nginx -t && sudo systemctl reload nginx
    echo "  Installed nginx config for $DOMAIN (HTTP only)"
    echo "  To enable HTTPS, run: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DOMAIN"
else
    echo "  nginx config already exists, skipping"
fi

echo ""
echo "Setup complete. Next steps:"
echo "  1. Edit /opt/leanfin/.env with your values"
echo "  2. From your dev machine, run: ./deploy.sh deploy"
echo "  3. Create a user: sudo -u leanfin /opt/leanfin/leanfin create-user --username <name> --password <pass>"
echo "  4. Set up HTTPS: sudo apt install python3-certbot-nginx && sudo certbot --nginx -d $DOMAIN"
SETUP
}

# ── Command dispatch ───────────────────────────────────────────────
case "${1}" in
    build)   build ;;
    deploy)  build && install && restart ;;
    setup)   setup ;;
    restart) restart ;;
    logs)    ssh -t "$SERVER" "sudo journalctl -u leanfin -f --no-pager" ;;
    status)  ssh -t "$SERVER" "sudo systemctl --no-pager status leanfin" ;;
    *)       usage ;;
esac
