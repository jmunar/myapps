#!/usr/bin/env bash
#
# Compose an `msb` command line from a profile and capabilities.
#
# Everything the sandbox may do is expressed as command-line flags rather than a
# config file: `msb --conf` takes a "sparse single-sandbox configuration" whose
# schema is not something the installed binary will tell you, while `msb run
# --help` documents every flag exactly. One source of truth beats two.
#
# No YAML parser is involved either. The schema is small and fixed, so a
# capability is a shell fragment that calls the directives below, and sourcing it
# *is* parsing. Merging is appending.
#
# Directives a fragment may call:
#
#   describe TEXT             one line, shown by `devbox.sh capabilities`
#   conflicts NAME...         capabilities that must not be granted alongside
#   image REF                 msb image reference (see `devbox.sh build-image`)
#   cpus|memory|workdir|user|hostname VALUE
#   profile NAME              an msb network profile (public, private, host);
#                             the allow-list model does not use one
#   env NAME VALUE            later fragments win
#   mount SPEC                SOURCE:DEST[:OPTIONS]; OPTIONS may include `ro`
#                             and `quota=<MiB>` (msb's default is 4096)
#   allow TARGET...           any allow rule makes egress deny-by-default; a
#                             target may be a host, a group (`host` is the
#                             machine msb runs on), and may carry `:tcp:<port>`
#   deny TARGET...            emitted before the allows: first match wins
#   port SPEC                 HOST:GUEST or BIND_ADDR:HOST:GUEST
#   secret NAME HOST...       msb reads $NAME on the host and substitutes it
#                             into requests to those hosts only
#   secret_file NAME PATH     where the host keeps that value
#   broker NAME               a host broker that must be running
#   cli ARG...                any other msb flag, verbatim
#
# Fragments read the sandbox's paths from the environment: BRANCH, CLONE,
# TARGET_CACHE, CARGO_CACHE, MODELS, CLAUDE_STATE, BROKER_PORT, PROD_PORT,
# BROKER_TOKEN, HOST_ALIAS, GITHUB_TOKEN_FILE.
set -euo pipefail

SANDBOX_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Defaulted so `--describe` can source a fragment without a sandbox to render;
# the render path checks below that the ones it needs are actually set.
: "${BRANCH:=}" "${CLONE:=}" "${TARGET_CACHE:=}" "${CARGO_CACHE:=}" "${MODELS:=}"
: "${BROKER_PORT:=}" "${PROD_PORT:=}" "${BROKER_TOKEN:=}" "${HOST_ALIAS:=}"
: "${GITHUB_TOKEN_FILE:=}" "${CLAUDE_STATE:=}"

profile="default"
caps=""
out_dir=""
describe_only=""

while [ $# -gt 0 ]; do
    case "$1" in
        --profile) profile="$2"; shift 2 ;;
        --caps) caps="$2"; shift 2 ;;
        --out-dir) out_dir="$2"; shift 2 ;;
        --describe) describe_only="$2"; shift 2 ;;
        *) echo "render: unknown argument $1" >&2; exit 1 ;;
    esac
done

die() { echo "render: $*" >&2; exit 1; }

# --- state the directives accumulate into ----------------------------------
CFG_IMAGE=""; CFG_CPUS=""; CFG_MEMORY=""; CFG_WORKDIR=""; CFG_USER=""
CFG_HOSTNAME=""; NET_PROFILE=""
declare -A ENV_MAP=()
ENV_ORDER=(); MOUNTS=(); ALLOW=(); DENY=(); PORTS=(); EXTRA=(); BROKERS=()
SECRET_NAMES=(); SECRET_HOSTS=(); SECRET_FILES=()
FRAG_DESCRIPTION=""; FRAG_CONFLICTS=()

describe() { FRAG_DESCRIPTION="$*"; }
conflicts() { FRAG_CONFLICTS+=("$@"); }
image()    { CFG_IMAGE="$1"; }
cpus()     { CFG_CPUS="$1"; }
memory()   { CFG_MEMORY="$1"; }
workdir()  { CFG_WORKDIR="$1"; }
user()     { CFG_USER="$1"; }
hostname() { CFG_HOSTNAME="$1"; }
profile()  { NET_PROFILE="$1"; }
mount()    { MOUNTS+=("$1"); }
allow()    { ALLOW+=("$@"); }
deny()     { DENY+=("$@"); }
port()     { PORTS+=("$1"); }
cli()      { EXTRA+=("$@"); }
broker()   { BROKERS+=("$1"); }

env() {
    local name="$1"; shift
    [ -n "${ENV_MAP[$name]+set}" ] || ENV_ORDER+=("$name")
    ENV_MAP["$name"]="$*"
}

secret() {
    local name="$1"; shift
    SECRET_NAMES+=("$name")
    # msb wants ENV[:OPTIONS]@HOST[,HOST...]
    SECRET_HOSTS+=("$(IFS=,; echo "$*")")
}

secret_file() { SECRET_FILES+=("$1=$2"); }

dedup() {
    local seen=() item
    for item in "$@"; do
        case " ${seen[*]-} " in *" $item "*) continue ;; esac
        seen+=("$item")
        printf '%s\n' "$item"
    done
}

fragment_path() {
    local name="$1" kind="$2" path
    path="$SANDBOX_DIR/$kind/$name.sh"
    [ -f "$path" ] || die "no such $kind: $name"
    printf '%s' "$path"
}

# --- describe one fragment and stop ----------------------------------------

if [ -n "$describe_only" ]; then
    # shellcheck disable=SC1090
    source "$(fragment_path "$describe_only" capabilities)"
    printf '%s\n' "$FRAG_DESCRIPTION"
    exit 0
fi

[ -n "$out_dir" ] || die "--out-dir is required"
for required in BRANCH CLONE TARGET_CACHE MODELS CLAUDE_STATE; do
    [ -n "${!required}" ] || die "$required is not set"
done
mkdir -p "$out_dir"

# --- load ------------------------------------------------------------------

granted=()
IFS=',' read -r -a granted <<< "$caps"

# shellcheck disable=SC1090
source "$(fragment_path "$profile" profiles)"

for cap in "${granted[@]-}"; do
    [ -n "$cap" ] || continue
    FRAG_CONFLICTS=()
    # shellcheck disable=SC1090
    source "$(fragment_path "$cap" capabilities)"
    # Lists concatenate, so two capabilities that both set the same thing would
    # silently produce both — a sandbox publishing a port twice, once to
    # loopback and once to the LAN. Conflicts are declared, not detected.
    for other in "${FRAG_CONFLICTS[@]-}"; do
        [ -n "$other" ] || continue
        case ",$caps," in *",$other,"*) die "capability '$cap' conflicts with '$other' — revoke one" ;; esac
    done
done

[ -n "$CFG_IMAGE" ] || die "no image: the profile must set one"

# --- emit the command line -------------------------------------------------

ARGS=()
add() { ARGS+=("$@"); }

[ -n "$CFG_CPUS" ]     && add --cpus "$CFG_CPUS"
[ -n "$CFG_MEMORY" ]   && add --memory "$CFG_MEMORY"
[ -n "$CFG_WORKDIR" ]  && add --workdir "$CFG_WORKDIR"
[ -n "$CFG_USER" ]     && add --user "$CFG_USER"
[ -n "$CFG_HOSTNAME" ] && add --hostname "$CFG_HOSTNAME"

for name in "${ENV_ORDER[@]-}"; do
    [ -n "$name" ] || continue
    add --env "$name=${ENV_MAP[$name]}"
done

# msb puts a quota on every directory-backed mount, and defaults it to
# 4096 MiB when none is given — enough to fail a debug build of this workspace
# partway through. `--volume` has no way to set it; only `--mount-dir` takes
# `quota=`. So a mount that declares a quota has to go out as --mount-dir.
# Both accept `ro`, and a mount with no quota keeps msb's 4 GiB default.
while read -r item; do
    [ -n "$item" ] || continue
    case "$item" in
        *quota=*) add --mount-dir "$item" ;;
        *)        add --volume "$item" ;;
    esac
done < <(dedup "${MOUNTS[@]-}")
while read -r item; do [ -n "$item" ] && add --port "$item"; done < <(dedup "${PORTS[@]-}")

# Network. msb's egress default is already deny once any --net-rule is present
# (with an implicit allow@public only when there are none), but saying it out
# loud costs one flag and removes the "what happens if this list is empty"
# question from every reading of this file.
if [ "${#ALLOW[@]}" -eq 0 ] && [ "${#PORTS[@]}" -eq 0 ]; then
    # Nothing was granted that needs the network — but NOT `--no-net`: on
    # msb 0.7.2 that leaves the guest agent unreachable too, and every
    # `msb exec` against the sandbox then hangs forever. Deny-by-default egress
    # with no allow rules is the same thing for the workload, without the wedge.
    add --net-default-egress deny
else
    add --net-default-egress deny
    [ -n "$NET_PROFILE" ] && add --net "$NET_PROFILE"
    # Denies first: the first matching rule wins.
    while read -r item; do [ -n "$item" ] && add --net-rule "deny@$item"; done < <(dedup "${DENY[@]-}")
    if [ "${#ALLOW[@]}" -gt 0 ]; then
        # Explicit rules do not carry the DNS access a profile would.
        add --net-rule "allow@dns"
        while read -r item; do [ -n "$item" ] && add --net-rule "allow@$item"; done < <(dedup "${ALLOW[@]-}")
    fi
fi

# Secrets. Substitution happens in the TLS proxy, so a secret without
# interception would simply never be injected.
if [ "${#SECRET_NAMES[@]}" -gt 0 ]; then
    add --tls-intercept
    add --secret-violation-action block-and-log
    for i in "${!SECRET_NAMES[@]}"; do
        add --secret "${SECRET_NAMES[$i]}@${SECRET_HOSTS[$i]}"
    done
fi

add "${EXTRA[@]-}"

# The image is positional and goes last.
add "$CFG_IMAGE"

: > "$out_dir/msb-args"
for arg in "${ARGS[@]}"; do
    [ -n "$arg" ] && printf '%s\n' "$arg" >> "$out_dir/msb-args"
done

printf '%s\n' "${SECRET_FILES[@]-}" | grep -v '^$' > "$out_dir/secret-files" || true
if [ "${#BROKERS[@]}" -gt 0 ]; then
    dedup "${BROKERS[@]}" > "$out_dir/brokers"
else
    : > "$out_dir/brokers"
fi
