#!/usr/bin/env bash
#
# Build the app, start a temporary server with seeded demo data,
# run the Playwright screenshot script, and clean up.
#
# Usage:  ./scripts/take-screenshots.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DEMO_USER="${SCREENSHOT_USER:-demo}"
DEMO_PASS="${SCREENSHOT_PASS:-demo}"
DB_FILE="data/screenshots.db"
BIND_ADDR="127.0.0.1:3199"
BASE_URL="http://${BIND_ADDR}"
OUT_DIR="docs/screenshots"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    echo ":: Stopping server (pid $SERVER_PID)..."
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -f "$DB_FILE" "${DB_FILE}-wal" "${DB_FILE}-shm"
}
trap cleanup EXIT

# ── 1. Build ──
echo ":: Building (release)..."
cargo build --release 2>&1

# ── 2. Prepare database & seed ──
echo ":: Creating demo user and seeding data..."
rm -f "$DB_FILE" "${DB_FILE}-wal" "${DB_FILE}-shm"

export DATABASE_URL="sqlite://${DB_FILE}"
export BIND_ADDR
export BASE_URL
export ENCRYPTION_KEY="0000000000000000000000000000000000000000000000000000000000000000"

./target/release/myapps create-user --username "$DEMO_USER" --password "$DEMO_PASS"
./target/release/myapps seed --user "$DEMO_USER"

# ── 3. Start server in background ──
echo ":: Starting server on ${BIND_ADDR}..."
./target/release/myapps serve &
SERVER_PID=$!

# Wait for the server to be ready.
for i in $(seq 1 30); do
  if curl -sf "${BASE_URL}/login" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "ERROR: Server exited prematurely." >&2
    exit 1
  fi
  sleep 0.5
done

# ── 4. Install Playwright (if needed) & take screenshots ──
echo ":: Installing Playwright dependencies..."
if [[ ! -d node_modules ]]; then
  npm install --save-dev @playwright/test 2>&1
fi
npx playwright install chromium 2>&1

echo ":: Capturing screenshots..."
mkdir -p "$OUT_DIR"

SCREENSHOT_USER="$DEMO_USER" \
SCREENSHOT_PASS="$DEMO_PASS" \
BASE_URL="$BASE_URL" \
  npx playwright test scripts/screenshots.ts --reporter=list

echo ":: Done! Screenshots saved to ${OUT_DIR}/"
ls -1 "$OUT_DIR"/*.png
