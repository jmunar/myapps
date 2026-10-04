#!/usr/bin/env bash
#
# Compose a `bwrap` command line from a profile and capabilities.
#
# Everything the sandbox may do is expressed as arguments to one `bwrap`
# invocation, written to `.devbox/<branch>/bwrap-args`, plus a short `plan` of
# the host-side processes that have to exist around it. Read either one and you
# have read the whole of the sandbox's authority — there is no daemon holding
# state and no configuration file the kernel consults later.
#
# No YAML parser is involved. The schema is small and fixed, so a capability is
# a shell fragment that calls the directives below, and sourcing it *is*
# parsing. Merging is appending.
#
# Directives a fragment may call:
#
#   describe TEXT             one line, shown by `devbox.sh capabilities`
#   conflicts NAME...         capabilities that must not be granted alongside
#   cpus N                    CPUQuota for the scope the sandbox runs in
#   memory SIZE               MemoryMax for that scope (e.g. 8G)
#   workdir PATH              where the command starts
#   hostname NAME             the sandbox's own UTS name
#   env NAME VALUE            later fragments win
#   mount SPEC                SOURCE:DEST[:ro]; the source must already exist
#   allow HOST...             hostnames the egress proxy will connect to
#   port SPEC                 [BIND:]HOST_PORT:GUEST_PORT, published by socat
#   broker NAME               a host broker to start (anthropic, prod)
#   credential ENV SOURCE     a secret to put in the sandbox's environment
#   cli ARG...                any other bwrap flag, verbatim
#
# What a fragment does *not* control is the base filesystem: the read-only /usr,
# the empty /etc, the tmpfs $HOME and the absence of a network. That lives below
# in `base_layout`, in one place, because getting it wrong is the one mistake
# this directory exists to prevent and it should not be editable per profile.
#
# Fragments read the sandbox's paths from the environment: BRANCH, CLONE,
# TARGET_CACHE, CARGO_HOME_DIR, CARGO_CACHE, MODELS, CLAUDE_STATE, GUEST_BIN,
# BOOTSTRAP_DIR, HOME_DIR, PLAYWRIGHT_CACHE, RUNTIME_DIR, and the three fixed
# in-sandbox ports ANTHROPIC_PORT, PROD_PORT, PROXY_PORT.
set -euo pipefail

SANDBOX_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Defaulted so `--describe` can source a fragment without a sandbox to render;
# the render path checks below that the ones it needs are actually set.
: "${BRANCH:=}" "${CLONE:=}" "${TARGET_CACHE:=}" "${CARGO_CACHE:=}" "${MODELS:=}"
: "${CARGO_HOME_DIR:=}" "${CLAUDE_STATE:=}" "${GUEST_BIN:=}" "${HOME_DIR:=$HOME}"
: "${PLAYWRIGHT_CACHE:=}" "${RUNTIME_DIR:=}" "${BOOTSTRAP_DIR:=}"
# Ports inside the sandbox. They are constants, not claimed: each sandbox has
# its own network namespace, so two sandboxes both listening on 8080 never meet.
: "${ANTHROPIC_PORT:=8080}" "${PROD_PORT:=8081}" "${PROXY_PORT:=3128}"

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
CFG_CPUS=""; CFG_MEMORY=""; CFG_WORKDIR=""; CFG_HOSTNAME=""
declare -A ENV_MAP=()
ENV_ORDER=(); MOUNTS=(); ALLOW=(); PORTS=(); EXTRA=(); BROKERS=(); CREDENTIALS=()
FRAG_DESCRIPTION=""; FRAG_CONFLICTS=()

describe() { FRAG_DESCRIPTION="$*"; }
conflicts() { FRAG_CONFLICTS+=("$@"); }
cpus()     { CFG_CPUS="$1"; }
memory()   { CFG_MEMORY="$1"; }
workdir()  { CFG_WORKDIR="$1"; }
hostname() { CFG_HOSTNAME="$1"; }
mount()    { MOUNTS+=("$1"); }
allow()    { ALLOW+=("$@"); }
port()     { PORTS+=("$1"); }
cli()      { EXTRA+=("$@"); }
broker()   { BROKERS+=("$1"); }
credential() { CREDENTIALS+=("$1=$2"); }

env() {
    local name="$1"; shift
    [ -n "${ENV_MAP[$name]+set}" ] || ENV_ORDER+=("$name")
    ENV_MAP["$name"]="$*"
}

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
for required in BRANCH CLONE TARGET_CACHE CARGO_HOME_DIR MODELS CLAUDE_STATE \
                GUEST_BIN BOOTSTRAP_DIR RUNTIME_DIR; do
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

# --- emit the command line -------------------------------------------------

ARGS=()
add() { ARGS+=("$@"); }

# A path the host may or may not have. bwrap's own --*-try flags would do this,
# but resolving it here means `devbox.sh config` shows what was actually bound.
add_if_exists() {
    local flag="$1" source="$2" dest="${3:-$2}"
    [ -e "$source" ] && add "$flag" "$source" "$dest"
    return 0
}

# Files under /etc the toolchain genuinely reads. An `--ro-bind /etc /etc`
# would be one line instead of this list, and would also hand the sandbox
# every world-readable configuration file on the machine — which is the
# category the host's own credentials keep turning up in.
#
# /etc/resolv.conf is *deliberately* absent: there is no network namespace to
# resolve in, every allowed name is resolved by the proxy on the host, and a
# resolver that fails immediately is easier to diagnose than one that hangs.
ETC_ENTRIES=(
    ld.so.cache ld.so.conf ld.so.conf.d
    ssl ca-certificates pki crypto-policies
    passwd group nsswitch.conf localtime alternatives
    profile profile.d bash.bashrc bashrc inputrc
    gitconfig
)

base_layout() {
    # One namespace of each kind. --unshare-net is what makes the egress proxy
    # the only way out rather than merely the recommended one.
    add --unshare-user --unshare-ipc --unshare-pid --unshare-uts --unshare-cgroup --unshare-net
    # SIGKILL when the terminal that started it goes away: without this a
    # background build outlives the session that could stop it.
    add --die-with-parent
    # Deliberately *not* --new-session. It calls setsid(), which costs the
    # sandbox its controlling terminal — no job control, no Ctrl-C, and Claude
    # Code's TUI misbehaves. Its purpose is to block TIOCSTI input injection
    # back into the parent's terminal, and that syscall is disabled by default
    # since Linux 6.2 (`dev.tty.legacy_tiocsti`), which `devbox.sh doctor`
    # checks. On a kernel where it is enabled, doctor says so.

    # The sandbox's environment is built from nothing. What it gets is the
    # `env` file below, written into the runtime directory at 0600 and sourced
    # by the entrypoint — not `--setenv`, because the GitHub token would then
    # be in an argument vector and /proc/<pid>/cmdline is world-readable.
    add --clearenv

    add --tmpfs /
    add --ro-bind /usr /usr
    # Wherever the host has merged /usr, the sandbox does too.
    local link
    for dir in /bin /sbin /lib /lib64; do
        if link="$(readlink "$dir" 2>/dev/null)"; then
            add --symlink "$link" "$dir"
        elif [ -d "$dir" ]; then
            add --ro-bind "$dir" "$dir"
        fi
    done

    add --dir /etc
    local entry
    for entry in "${ETC_ENTRIES[@]}"; do
        add_if_exists --ro-bind "/etc/$entry"
    done
    # Generated rather than bound: the host's /etc/hosts names its LAN, and the
    # sandbox has no business learning what is on it.
    add --ro-bind "$out_dir/hosts" /etc/hosts

    add --proc /proc
    add --dev /dev
    add --tmpfs /tmp
    add --dir /var
    add --tmpfs /var/tmp

    # $HOME exists and is writable, and holds nothing except what is mounted
    # into it below. Everything a tool scatters there is gone with the sandbox.
    add --tmpfs "$HOME_DIR"
    add --dir "$HOME_DIR/.local/bin"
    add --dir "$HOME_DIR/.config"

    # The host's toolchain, read-only. There is no guest image to build and no
    # second copy of Rust on the disk; the price is that the sandbox and the
    # host compile with the same rustc, which is also why their target caches
    # are interchangeable.
    add_if_exists --ro-bind "$HOME/.rustup" "$HOME_DIR/.rustup"
    add_if_exists --ro-bind "$HOME/.cargo/bin" /opt/devbox/cargo-bin
    add_if_exists --ro-bind "$HOME/.local/share/claude" "$HOME_DIR/.local/share/claude"
    local claude_bin
    if claude_bin="$(readlink -f "$(command -v claude 2>/dev/null)" 2>/dev/null)" \
        && [ -n "$claude_bin" ]; then
        add --symlink "$claude_bin" "$HOME_DIR/.local/bin/claude"
    fi
    # devbox-prod and devbox-git-askpass, which used to be baked into the guest
    # image, and the first-boot/seed scripts. Read-only, so a sandbox cannot
    # rewrite the helper it is about to be handed.
    add --ro-bind "$GUEST_BIN" /opt/devbox/bin
    add --ro-bind "$BOOTSTRAP_DIR" /opt/devbox/bootstrap

    # The socket directory: the brokers' and the proxy's sockets, and the only
    # channel out of the namespace. Which sandbox can reach which broker is
    # this line and nothing else.
    add --bind "$RUNTIME_DIR" /run/devbox
}

base_layout

[ -n "$CFG_HOSTNAME" ] && add --hostname "$CFG_HOSTNAME"

# Mounts, in the order they were declared: a mount nested inside another (the
# shared cargo registry inside $HOME/.cargo) only works if its parent went
# first, and fragments are applied after the profile for exactly that reason.
while read -r item; do
    [ -n "$item" ] || continue
    mount_source="${item%%:*}"
    rest="${item#*:}"
    # Checked here rather than left to bwrap: its own error names the
    # destination, which is the half you did not write.
    [ -e "$mount_source" ] || die "mount source does not exist: $mount_source"
    case "$rest" in
        *:ro) add --ro-bind "$mount_source" "${rest%:ro}" ;;
        *)    add --bind "$mount_source" "$rest" ;;
    esac
done < <(dedup "${MOUNTS[@]-}")

[ -n "$CFG_WORKDIR" ] && add --chdir "$CFG_WORKDIR"
add "${EXTRA[@]-}"

: > "$out_dir/bwrap-args"
for arg in "${ARGS[@]}"; do
    [ -n "$arg" ] && printf '%s\n' "$arg" >> "$out_dir/bwrap-args"
done

# --- the environment -------------------------------------------------------
#
# The static half. devbox.sh copies this into the sandbox's runtime directory,
# adds the terminal's own variables and whatever credentials the plan asks for,
# and the entrypoint sources the result.
{
    printf 'HOME=%s\n' "$HOME_DIR"
    printf 'PATH=%s\n' "$HOME_DIR/.cargo/bin:/opt/devbox/cargo-bin:$HOME_DIR/.local/bin:/opt/devbox/bin:/usr/local/bin:/usr/bin:/bin"
    printf 'CARGO_HOME=%s\n' "$HOME_DIR/.cargo"
    printf 'RUSTUP_HOME=%s\n' "$HOME_DIR/.rustup"
    # Loopback is the brokers; sending it to the proxy would be a loop.
    printf 'NO_PROXY=%s\nno_proxy=%s\n' "127.0.0.1,localhost" "127.0.0.1,localhost"
    if [ "${#ALLOW[@]}" -gt 0 ]; then
        printf 'HTTPS_PROXY=http://127.0.0.1:%s\n' "$PROXY_PORT"
        printf 'https_proxy=http://127.0.0.1:%s\n' "$PROXY_PORT"
    fi
    for name in "${ENV_ORDER[@]-}"; do
        [ -n "$name" ] || continue
        printf '%s=%s\n' "$name" "${ENV_MAP[$name]}"
    done
} > "$out_dir/env"

# --- /etc/hosts ------------------------------------------------------------
cat > "$out_dir/hosts" <<HOSTS
127.0.0.1	localhost	${CFG_HOSTNAME:-sandbox}
::1	localhost	ip6-localhost	ip6-loopback
HOSTS

# --- the plan --------------------------------------------------------------
#
# Everything that has to exist on the host *around* the sandbox: the brokers,
# the proxy, the relays in both directions, the secrets to fetch, the resource
# limits to put on the scope. devbox.sh reads this; nothing else does.
{
    while read -r item; do
        [ -n "$item" ] || continue
        case "$item" in
            anthropic) printf 'broker anthropic anthropic.sock %s\n' "$ANTHROPIC_PORT" ;;
            prod)      printf 'broker prod prod.sock %s\n' "$PROD_PORT" ;;
            *)         echo "render: unknown broker '$item'" >&2; exit 1 ;;
        esac
    done < <(dedup "${BROKERS[@]-}")

    # No allow-list, no proxy: a sandbox that was granted nothing that needs
    # the network does not get a way out at all, rather than an empty one.
    if [ "${#ALLOW[@]}" -gt 0 ]; then
        printf 'proxy proxy.sock %s %s\n' "$PROXY_PORT" \
            "$(dedup "${ALLOW[@]}" | paste -sd, -)"
    fi

    while read -r item; do
        [ -n "$item" ] || continue
        # [BIND:]HOST_PORT:GUEST_PORT
        local_bind="127.0.0.1"
        case "$(tr -dc ':' <<< "$item" | wc -c)" in
            2) local_bind="${item%%:*}"; item="${item#*:}" ;;
        esac
        printf 'publish %s %s %s\n' "$local_bind" "${item%%:*}" "${item##*:}"
    done < <(dedup "${PORTS[@]-}")

    printf '%s\n' "${CREDENTIALS[@]-}" | grep -v '^$' | sed 's/^/credential /' || true

    [ -n "$CFG_CPUS" ] && printf 'limit cpus %s\n' "$CFG_CPUS"
    [ -n "$CFG_MEMORY" ] && printf 'limit memory %s\n' "$CFG_MEMORY"
    true
} > "$out_dir/plan"
