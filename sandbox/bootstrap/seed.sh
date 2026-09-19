#!/usr/bin/env bash
#
# Runs inside the guest: build once, then create a dev user and seed it.
# Separate from first-boot because it needs a full compile, and creating a
# sandbox should stay fast.
set -euo pipefail

cd /workspace

user="${1:-dev}"
password="${2:-dev}"

# VAPID keys are optional in Config, but Web Push is silently off without them,
# and generating them needs the binary — hence "after the first build".
#
# `generate-vapid-keys` prints a human preamble before the three lines, and
# .env.example already carries the keys empty, so this replaces them in place
# rather than appending the output.
if ! grep -qE '^VAPID_PRIVATE_KEY=.+' .env; then
    echo "seed: generating VAPID keys"
    while IFS='=' read -r key value; do
        [ -n "$key" ] || continue
        value="${value//&/\\&}"
        if grep -qE "^#?${key}=" .env; then
            sed -i -E "s|^#?${key}=.*|${key}=${value}|" .env
        else
            printf '%s=%s\n' "$key" "$value" >> .env
        fi
    done < <(cargo run --quiet -- generate-vapid-keys | grep -E '^VAPID_')
fi

cargo run --quiet -- create-user --username "$user" --password "$password"
cargo run --quiet -- seed --user "$user"

echo "seed: user '$user' created and seeded (password: '$password')"
