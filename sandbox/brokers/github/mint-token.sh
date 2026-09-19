#!/usr/bin/env bash
#
# Mint a short-lived GitHub App installation token on the host.
#
# Fine-grained PATs cannot be created from a script — GitHub's API can list,
# approve and revoke them but not create one, and `gh` has no command for it.
# A GitHub App can: created once in the UI, it then mints tokens on demand that
# expire an hour later. That is the difference between a static credential in a
# config file and one whose leak is worth an hour.
#
# Config: ~/.config/msb-devbox/github-app.json
#   {
#     "app_id": "123456",
#     "installation_id": "78901234",
#     "private_key": "~/.config/msb-devbox/github-app.pem",
#     "repositories": ["myapps"],
#     "permissions": {
#       "contents": "write",
#       "pull_requests": "write",
#       "workflows": "write",
#       "actions": "read",
#       "checks": "read"
#     }
#   }
#
# Prints the token on stdout and nothing else.
set -euo pipefail

config="${1:-${XDG_CONFIG_HOME:-$HOME/.config}/msb-devbox/github-app.json}"
[ -f "$config" ] || { echo "mint-token: no such config: $config" >&2; exit 1; }

app_id=$(jq -r '.app_id' "$config")
installation_id=$(jq -r '.installation_id' "$config")
key_path=$(jq -r '.private_key' "$config")
key_path="${key_path/#\~/$HOME}"
[ -f "$key_path" ] || { echo "mint-token: no private key at $key_path" >&2; exit 1; }

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

now=$(date +%s)
# iat backdated by 60s to survive clock skew; GitHub rejects a JWT older than
# 10 minutes, so 9 is the practical ceiling for exp.
header='{"alg":"RS256","typ":"JWT"}'
payload=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' "$((now - 60))" "$((now + 540))" "$app_id")

signing_input="$(printf '%s' "$header" | b64url).$(printf '%s' "$payload" | b64url)"
signature=$(printf '%s' "$signing_input" | openssl dgst -sha256 -sign "$key_path" | b64url)
jwt="$signing_input.$signature"

body=$(jq -n \
    --argjson repositories "$(jq -c '.repositories // []' "$config")" \
    --argjson permissions "$(jq -c '.permissions // {}' "$config")" \
    '{repositories: $repositories, permissions: $permissions}
     | with_entries(select(.value != [] and .value != {}))')

response=$(curl -fsS -X POST \
    -H "Authorization: Bearer $jwt" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    -d "$body" \
    "https://api.github.com/app/installations/${installation_id}/access_tokens")

jq -er '.token' <<<"$response"
