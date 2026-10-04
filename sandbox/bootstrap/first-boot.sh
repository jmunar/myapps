#!/usr/bin/env bash
#
# Runs inside the guest, once, right after the sandbox is created.
#
# Every secret it writes is generated here and worth nothing outside this VM.
# The host's real .env, deploy/*.env and data/ are never mounted, so this is
# what makes the checkout runnable at all.
#
# Bind-mounted read-only at /opt/devbox/bootstrap and run by `devbox.sh`, so
# changing it takes effect on the next command — there is no image to rebuild.
set -euo pipefail

cd /workspace

if [ -f .env ]; then
    echo "first-boot: .env already present, nothing to do"
    exit 0
fi

cp .env.example .env

set_var() {
    local key="$1" value="$2"
    # `|` as the delimiter because every value here contains a slash or a colon,
    # and `&` escaped because sed reads it as "the whole match".
    value="${value//&/\\&}"
    # Replace the line if the key exists (commented or not), else append.
    if grep -qE "^#?${key}=" .env; then
        sed -i -E "s|^#?${key}=.*|${key}=${value}|" .env
    else
        printf '%s=%s\n' "$key" "$value" >> .env
    fi
}

set_var ENCRYPTION_KEY "$(openssl rand -hex 32)"
set_var DATABASE_URL "sqlite://data/myapps.db"
set_var FILE_CLIPBOARD_DIR "data/file_clipboard"
# Loopback, deliberately: the `preview` capability relays to it from the host
# over a Unix socket, so the server never has to listen beyond the sandbox.
set_var BIND_ADDR "127.0.0.1:3000"
set_var BASE_URL "http://localhost:3000"
set_var WHISPER_MODELS_DIR "models"

mkdir -p data/file_clipboard

echo "first-boot: wrote a throwaway .env and created data/"
echo "first-boot: run './devbox.sh seed <branch>' for a dev user and seed data"
