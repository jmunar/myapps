#!/usr/bin/env bash
#
# Development in a sandbox: one per branch, no credentials inside it.
#
# The host keeps ~/.claude, the GitHub credential, ~/.ssh, the real .env and the
# real data/. The sandbox gets a clone of the repository and the host's own
# toolchain, read-only, in a set of namespaces with no network in them. What
# crosses the boundary is exactly what the granted capabilities say, and the
# generated .devbox/<branch>/bwrap-args and plan are the whole of it.
#
# There is no daemon and no lifecycle: a sandbox exists for as long as a command
# is running in it. Everything it needs on the host — the brokers, the egress
# proxy, the relays — starts with that command and dies with it.
#
# Usage: sandbox/README.md. Changing any of this: sandbox/CLAUDE.md.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SANDBOX_DIR="$REPO_DIR/sandbox"
STATE_ROOT="$REPO_DIR/.devbox"
CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}/devbox"
LOG_ROOT="${XDG_STATE_HOME:-$HOME/.local/state}/devbox"
RUNTIME_ROOT="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/devbox"
BROKER_BIN="$SANDBOX_DIR/brokers/anthropic/target/release/devbox-broker-anthropic"
PROD_BROKER_BIN="$SANDBOX_DIR/brokers/prod/target/release/devbox-broker-prod"
PROXY_BIN="$SANDBOX_DIR/brokers/proxy/target/release/devbox-broker-proxy"
# Which deployment the prod broker reads, as a path relative to the repository.
# The same file deploy.sh reads, so prod is described in exactly one place.
PROD_ENV_FILE="${DEVBOX_PROD_ENV:-deploy/prod.env}"

# Ports *inside* the sandbox, which are constants rather than something to
# claim: each sandbox has its own network namespace, so two sandboxes both
# listening on 8080 never meet. Nothing on the host binds them.
export ANTHROPIC_PORT=8080 PROD_PORT=8081 PROXY_PORT=3128

# cargo-cache-shared is in the default set: without it every branch recompiles
# the whole dependency tree. It is also the one writable surface shared between
# sandboxes — revoke it for a branch whose dependencies you have not read.
DEFAULT_CAPS="${DEVBOX_CAPS:-anthropic,github,rust-deps,node-deps,cargo-cache-shared,preview}"
DEFAULT_PROFILE="${DEVBOX_PROFILE:-default}"

die() { echo "devbox: $*" >&2; exit 1; }
info() { echo "devbox: $*"; }
require() { command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed"; }

# Where the host keeps the GitHub credential. One path, no fallback: a
# credential that can live in two places is one you have to look for twice.
config_file() {
    printf '%s' "${XDG_CONFIG_HOME:-$HOME/.config}/devbox/$1"
}

clone_dir() { echo "$(dirname "$REPO_DIR")/myapps-$1"; }
state_dir() { echo "$STATE_ROOT/$1"; }

check_branch() {
    # No slashes: the branch name is also a directory name.
    [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] || die "refusing branch name '$1' (letters, digits, . _ - only)"
}

known_branch() {
    [ -d "$(state_dir "$1")" ] || die "no sandbox for '$1' (./devbox.sh list)"
}

# --- configuration ---------------------------------------------------------

# RUNTIME_DIR is per *invocation*, not per branch: two shells on one branch each
# get their own brokers and their own sockets, and neither can unlink the
# other's. It goes into bwrap-args, so the render is per invocation too — which
# costs nothing, since rendering is sourcing eight small shell fragments.
render() {
    local branch="$1" state models
    state="$(state_dir "$branch")"
    mkdir -p "$CACHE_ROOT/target/$branch" "$CACHE_ROOT/cargo-home/$branch" \
        "$CACHE_ROOT/cargo/registry" "$CACHE_ROOT/cargo/git" "$CACHE_ROOT/claude/$branch"
    mkdir -p "$REPO_DIR/models"
    models="$(cd "$REPO_DIR/models" && pwd -P)"

    BRANCH="$branch" \
    CLONE="$(clone_dir "$branch")" \
    TARGET_CACHE="$CACHE_ROOT/target/$branch" \
    CARGO_HOME_DIR="$CACHE_ROOT/cargo-home/$branch" \
    CARGO_CACHE="$CACHE_ROOT/cargo" \
    MODELS="$models" \
    CLAUDE_STATE="$CACHE_ROOT/claude/$branch" \
    GUEST_BIN="$SANDBOX_DIR/guest" \
    BOOTSTRAP_DIR="$SANDBOX_DIR/bootstrap" \
    PLAYWRIGHT_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/ms-playwright" \
    HOME_DIR="$HOME" \
    RUNTIME_DIR="${RUNTIME_DIR:-$RUNTIME_ROOT/$branch.config}" \
    bash "$SANDBOX_DIR/render.sh" \
        --profile "$(cat "$state/profile")" --caps "$(cat "$state/capabilities")" \
        --out-dir "$state"
}

# --- credentials -----------------------------------------------------------

# Read on the host, exported only into the one subshell that becomes bwrap, and
# never written to a file or an argument vector.
mint_credential() {
    local source="$1" app pat
    case "$source" in
        github)
            app="$(config_file github-app.json)"
            pat="$(config_file github-token)"
            if [ -f "$app" ]; then
                # An App installation token expires in an hour; a PAT does not.
                "$SANDBOX_DIR/brokers/github/mint-token.sh" "$app" \
                    || die "minting a GitHub App token failed"
            elif [ -f "$pat" ]; then
                tr -d '\r\n' < "$pat"
            else
                die "the github capability needs $app or $pat (see sandbox/README.md)"
            fi
            ;;
        *) die "unknown credential source '$source'" ;;
    esac
}

# --- running a sandbox -----------------------------------------------------

# Everything the host has to have running around the sandbox, torn down with it.
CHILD_PIDS=()
RUNTIME_DIR=""

cleanup_sandbox() {
    local pid
    for pid in "${CHILD_PIDS[@]-}"; do
        [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
    done
    [ -n "$RUNTIME_DIR" ] && rm -rf "$RUNTIME_DIR"
    return 0
}

start_helper() {
    local name="$1"; shift
    mkdir -p "$LOG_ROOT"
    "$@" >>"$LOG_ROOT/$name.log" 2>&1 &
    local pid=$!
    CHILD_PIDS+=("$pid")
    # A helper that dies on start is the difference between "Claude Code cannot
    # authenticate" and a one-line reason, so it is checked rather than hoped.
    sleep 0.2
    if ! kill -0 "$pid" 2>/dev/null; then
        [ -n "${HELPER_MAY_FAIL:-}" ] && return 1
        die "$name died on start — see $LOG_ROOT/$name.log"
    fi
    return 0
}

wait_for_socket() {
    local path="$1" attempt
    for attempt in $(seq 1 50); do
        [ -S "$path" ] && return 0
        sleep 0.1
    done
    die "$(basename "$path") never appeared — see $LOG_ROOT"
}

# Run a command in the sandbox. This is the whole of the lifecycle.
sandbox_run() {
    local branch="$1"; shift
    known_branch "$branch"
    require bwrap; require socat

    RUNTIME_DIR="$RUNTIME_ROOT/$branch.$$"
    mkdir -p "$RUNTIME_DIR"
    chmod 700 "$RUNTIME_DIR"
    trap cleanup_sandbox EXIT INT TERM

    render "$branch"
    local state; state="$(state_dir "$branch")"

    # --- the plan: host-side helpers, and the relays inside ----------------
    local -a inner_relays=() cred_exports=()
    local cpus="" memory=""
    local kind a b c
    while read -r kind a b c; do
        case "$kind" in
            broker)
                case "$a" in
                    anthropic)
                        [ -x "$BROKER_BIN" ] || die "broker not built — run ./devbox.sh build-broker"
                        start_helper "$branch-anthropic" "$BROKER_BIN" \
                            --listen-unix "$RUNTIME_DIR/$b" --sandbox "$branch" \
                            --audit-log "$LOG_ROOT/audit.jsonl" ${DEVBOX_BROKER_ARGS:-}
                        ;;
                    prod)
                        [ -x "$PROD_BROKER_BIN" ] || die "prod broker not built — run ./devbox.sh build-broker"
                        local env_file="$PROD_ENV_FILE"
                        case "$env_file" in /*) ;; *) env_file="$REPO_DIR/$env_file" ;; esac
                        [ -f "$env_file" ] || die "prod-readonly needs $env_file — see docs/deployment.md"
                        start_helper "$branch-prod" "$PROD_BROKER_BIN" \
                            --listen-unix "$RUNTIME_DIR/$b" --sandbox "$branch" \
                            --deploy-env "$env_file" --work-dir "$CACHE_ROOT/prod/$branch" \
                            --audit-log "$LOG_ROOT/audit.jsonl" ${DEVBOX_PROD_BROKER_ARGS:-}
                        ;;
                esac
                wait_for_socket "$RUNTIME_DIR/$b"
                inner_relays+=("socat TCP-LISTEN:$c,fork,reuseaddr,bind=127.0.0.1 UNIX-CONNECT:/run/devbox/$b")
                ;;
            proxy)
                [ -x "$PROXY_BIN" ] || die "proxy not built — run ./devbox.sh build-broker"
                start_helper "$branch-proxy" "$PROXY_BIN" \
                    --listen-unix "$RUNTIME_DIR/$a" --allow "$c" --sandbox "$branch" \
                    --audit-log "$LOG_ROOT/audit.jsonl" ${DEVBOX_PROXY_ARGS:-}
                wait_for_socket "$RUNTIME_DIR/$a"
                inner_relays+=("socat TCP-LISTEN:$b,fork,reuseaddr,bind=127.0.0.1 UNIX-CONNECT:/run/devbox/$a")
                ;;
            publish)
                # The other direction: the sandbox serves a socket, the host
                # listens on a port and hands it connections. The host side is
                # started even though the socket does not exist yet — socat
                # connects per connection, so it simply refuses until the dev
                # server is up.
                inner_relays+=("socat UNIX-LISTEN:/run/devbox/publish-$c.sock,fork,unlink-early TCP:127.0.0.1:$c")
                # Not fatal: a second shell on the same branch finds the port
                # taken by the first, and that is a reason to say so, not to
                # refuse to open a shell.
                if HELPER_MAY_FAIL=1 start_helper "$branch-publish-$c" \
                    socat "TCP-LISTEN:$b,fork,reuseaddr,bind=$a" \
                    "UNIX-CONNECT:$RUNTIME_DIR/publish-$c.sock"; then
                    info "publishing the dev server on $a:$b"
                else
                    info "warning: could not listen on $a:$b — see $LOG_ROOT/$branch-publish-$c.log"
                fi
                ;;
            credential)
                cred_exports+=("${a%%=*}=$(mint_credential "${a#*=}")")
                ;;
            limit)
                case "$a" in cpus) cpus="$b" ;; memory) memory="$b" ;; esac
                ;;
        esac
    done < "$state/plan"

    # --- the environment ---------------------------------------------------
    # A file rather than `--setenv`, because the GitHub token is in it and
    # /proc/<pid>/cmdline is world-readable. Mode 0600 in a 0700 directory that
    # only this sandbox has mounted.
    local env_file="$RUNTIME_DIR/env"
    ( umask 077; : > "$env_file" )
    quote_into_env() {
        # POSIX single-quote escaping: everything inside the quotes is literal
        # except a quote itself, which closes, escapes and reopens. Unquoted on
        # the right-hand side so the backslashes are the substitution's, not
        # the string's.
        local escaped=${2//\'/\'\\\'\'}
        printf "export %s='%s'\n" "$1" "$escaped" >> "$env_file"
    }
    local name value
    while IFS='=' read -r name value; do
        [ -n "$name" ] && quote_into_env "$name" "$value"
    done < "$state/env"
    # The terminal's own variables: without them the TUI draws in ASCII and
    # every accented character in the seed data comes out wrong.
    for name in TERM COLORTERM LANG LC_ALL LC_CTYPE LC_TIME TZ; do
        [ -n "${!name:-}" ] && quote_into_env "$name" "${!name}"
    done
    local cred
    for cred in "${cred_exports[@]-}"; do
        [ -n "$cred" ] && quote_into_env "${cred%%=*}" "${cred#*=}"
    done

    # --- the entrypoint ----------------------------------------------------
    # The relays run inside the namespace, started before the command and
    # reaped with it: bwrap is PID 1 in the sandbox's PID namespace, so when the
    # command exits the namespace goes and takes every socat with it.
    {
        echo '#!/bin/bash'
        echo '# Generated per invocation by devbox.sh. Not a file to edit.'
        echo 'set -u'
        echo '. /run/devbox/env'
        local relay
        for relay in "${inner_relays[@]-}"; do
            [ -n "$relay" ] && echo "$relay >/dev/null 2>&1 &"
        done
        echo 'exec "$@"'
    } > "$RUNTIME_DIR/entrypoint"
    chmod 0755 "$RUNTIME_DIR/entrypoint"

    local -a bwrap_args=()
    local line
    while IFS= read -r line; do
        [ -n "$line" ] && bwrap_args+=("$line")
    done < "$state/bwrap-args"

    # A systemd scope is how `cpus` and `memory` are enforced; without systemd
    # they are advisory and the sandbox is bounded by the host instead. It runs
    # outside the sandbox, with the host's own environment, which is why the
    # sandbox's environment is a file and not this process's.
    local -a scope=()
    if [ -z "${DEVBOX_NO_SCOPE:-}" ] && command -v systemd-run >/dev/null 2>&1; then
        scope=(systemd-run --user --scope --quiet --collect --unit "devbox-$branch-$$")
        [ -n "$memory" ] && scope+=(-p "MemoryMax=$memory")
        [ -n "$cpus" ] && scope+=(-p "CPUQuota=$((cpus * 100))%")
        scope+=(--)
    fi

    local status=0
    "${scope[@]}" bwrap "${bwrap_args[@]}" -- \
        /bin/bash /run/devbox/entrypoint "$@" || status=$?
    return $status
}

# --- commands --------------------------------------------------------------

cmd_create() {
    local branch="" base="main" caps="$DEFAULT_CAPS" profile="$DEFAULT_PROFILE"
    while [ $# -gt 0 ]; do
        case "$1" in
            --with) caps="$2"; shift 2 ;;
            --base) base="$2"; shift 2 ;;
            --profile) profile="$2"; shift 2 ;;
            -*) die "unknown flag $1" ;;
            *) branch="$1"; shift ;;
        esac
    done
    [ -n "$branch" ] || die "usage: devbox.sh create <branch> [--with caps] [--base ref]"
    check_branch "$branch"
    require git; require bwrap; require socat

    local clone state resume=0
    clone="$(clone_dir "$branch")"
    state="$(state_dir "$branch")"
    if [ -e "$clone" ]; then
        # A create that cannot be repeated turns every failure into a manual
        # cleanup. Reuse a clone this script made; refuse anything else.
        [ -d "$state" ] || die "$clone already exists and was not created by devbox"
        info "reusing the existing clone at $clone"
        resume=1
    fi

    if [ "$resume" -eq 0 ]; then
        git -C "$REPO_DIR" fetch --prune origin
        # --no-hardlinks is not hygiene: with hardlinks the clone's objects are
        # the same inodes as this repository's, and a sandbox that rewrites one
        # corrupts the host's copy.
        git clone --no-hardlinks --branch "$base" "$REPO_DIR" "$clone"
        git -C "$clone" checkout -b "$branch"

        # Point at GitHub over HTTPS: there is no SSH key in the sandbox by
        # design, and the egress proxy only speaks CONNECT to port 443.
        local origin
        origin="$(git -C "$REPO_DIR" remote get-url origin)"
        origin="${origin/git@github.com:/https://github.com/}"
        git -C "$clone" remote set-url origin "$origin"
        git -C "$clone" config user.name "$(git -C "$REPO_DIR" config user.name)"
        git -C "$clone" config user.email "$(git -C "$REPO_DIR" config user.email)"

        # Permissions granted inside the sandbox live in the clone; carry the
        # existing ones in so the first session is not a wall of prompts.
        if [ -f "$REPO_DIR/.claude/settings.local.json" ]; then
            mkdir -p "$clone/.claude"
            cp "$REPO_DIR/.claude/settings.local.json" "$clone/.claude/settings.local.json"
        fi
    fi

    mkdir -p "$state"
    echo "$caps" > "$state/capabilities"
    echo "$profile" > "$state/profile"
    render "$branch"

    info "running first-boot"
    sandbox_run "$branch" bash /opt/devbox/bootstrap/first-boot.sh

    echo
    info "sandbox ready:  ./devbox.sh claude $branch"
    info "capabilities:   $caps"
    info "config:         ./devbox.sh config $branch"
}

cmd_shell()  { sandbox_run "$1" bash -l; }
cmd_claude() { local b="$1"; shift; sandbox_run "$b" claude "$@"; }
cmd_exec()   { local b="$1"; shift; sandbox_run "$b" "$@"; }
cmd_seed()   { local b="$1"; shift; sandbox_run "$b" bash /opt/devbox/bootstrap/seed.sh "$@"; }

cmd_grant() {
    local branch="$1" cap="$2"
    known_branch "$branch"
    [ -f "$SANDBOX_DIR/capabilities/$cap.sh" ] || die "no such capability: $cap"
    local state caps
    state="$(state_dir "$branch")"
    caps="$(cat "$state/capabilities")"
    case ",$caps," in *",$cap,"*) info "$branch already has $cap"; return 0 ;; esac
    echo "$caps,$cap" > "$state/capabilities"
    # render refuses conflicting capabilities; put the old set back rather than
    # leaving the sandbox claiming a capability its config never got.
    if ! render "$branch"; then
        echo "$caps" > "$state/capabilities"
        die "did not grant $cap"
    fi
    info "granted $cap — it applies to the next command you run in $branch"
}

cmd_revoke() {
    local branch="$1" cap="$2"
    known_branch "$branch"
    local state kept="" item existing
    state="$(state_dir "$branch")"
    IFS=',' read -r -a existing <<< "$(cat "$state/capabilities")"
    for item in "${existing[@]}"; do
        { [ -z "$item" ] || [ "$item" = "$cap" ]; } && continue
        kept="${kept:+$kept,}$item"
    done
    echo "$kept" > "$state/capabilities"
    render "$branch"
    info "revoked $cap — it applies to the next command you run in $branch"
}

cmd_list() {
    [ -d "$STATE_ROOT" ] || { info "no sandboxes"; return 0; }
    printf '%-24s %-8s %-8s %s\n' BRANCH LIVE SIZE CAPABILITIES
    local dir branch size live
    for dir in "$STATE_ROOT"/*/; do
        [ -d "$dir" ] || continue
        branch="$(basename "$dir")"
        # "Live" is a count of running commands, not a state: nothing persists
        # between them.
        live="$(find "$RUNTIME_ROOT" -maxdepth 1 -name "$branch.[0-9]*" 2>/dev/null | wc -l)"
        size="$(du -sh "$CACHE_ROOT/target/$branch" 2>/dev/null | cut -f1)"
        printf '%-24s %-8s %-8s %s\n' "$branch" "$live" "${size:--}" "$(cat "$dir/capabilities")"
    done
}

cmd_capabilities() {
    local file name
    for file in "$SANDBOX_DIR/capabilities"/*.sh; do
        name="$(basename "$file" .sh)"
        printf '%-22s %s\n' "$name" "$(bash "$SANDBOX_DIR/render.sh" --describe "$name")"
    done
}

cmd_remove() {
    local branch="" force=0
    while [ $# -gt 0 ]; do
        case "$1" in
            --force|-f) force=1; shift ;;
            *) branch="$1"; shift ;;
        esac
    done
    [ -n "$branch" ] || die "usage: devbox.sh remove <branch> [--force]"
    known_branch "$branch"

    local clone state
    clone="$(clone_dir "$branch")"
    state="$(state_dir "$branch")"

    # Work in a sandbox lives in the clone and nowhere else. Losing it to a
    # tidy-up is the one failure this design could plausibly cause.
    if [ "$force" -eq 0 ] && [ -d "$clone" ]; then
        local unpushed dirty
        unpushed="$(git -C "$clone" log --oneline "@{upstream}..HEAD" 2>/dev/null | wc -l || echo 0)"
        if ! git -C "$clone" rev-parse '@{upstream}' >/dev/null 2>&1; then
            unpushed="$(git -C "$clone" log --oneline "origin/main..HEAD" 2>/dev/null | wc -l || echo 0)"
        fi
        dirty="$(git -C "$clone" status --porcelain | wc -l)"
        if [ "$unpushed" -gt 0 ] || [ "$dirty" -gt 0 ]; then
            die "$branch has $unpushed unpushed commit(s) and $dirty uncommitted change(s) — push them or pass --force"
        fi
    fi

    # Carry back permissions granted during the session.
    local wt_settings="$clone/.claude/settings.local.json"
    local main_settings="$REPO_DIR/.claude/settings.local.json"
    if [ -f "$wt_settings" ] && [ -f "$main_settings" ]; then
        jq -s '
            .[0] as $main | .[1] as $wt |
            $main * {permissions: {allow:
                (($main.permissions.allow // []) + ($wt.permissions.allow // []))
                | unique | sort
            }}
        ' "$main_settings" "$wt_settings" > "$main_settings.tmp" \
            && mv "$main_settings.tmp" "$main_settings" \
            && info "merged .claude/settings.local.json"
    fi

    rm -rf "$clone" "$state" "$CACHE_ROOT/target/$branch" "$CACHE_ROOT/claude/$branch" \
        "$CACHE_ROOT/cargo-home/$branch" "$CACHE_ROOT/prod/$branch"

    git -C "$REPO_DIR" fetch --prune origin >/dev/null 2>&1 || true
    if git -C "$REPO_DIR" ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
        info "branch '$branch' still on the remote — kept it"
    else
        git -C "$REPO_DIR" branch -D "$branch" 2>/dev/null \
            && info "deleted branch $branch" || true
    fi
    info "removed $branch"
}

cmd_build_broker() {
    require cargo
    local dir
    for dir in anthropic prod proxy; do
        (cd "$SANDBOX_DIR/brokers/$dir" && cargo build --release --quiet)
        info "built brokers/$dir"
    done
}

cmd_config() {
    local branch="$1"
    known_branch "$branch"
    render "$branch"
    local state; state="$(state_dir "$branch")"
    echo "# bwrap command line — RUNTIME_DIR is per invocation, shown here as .config"
    echo "bwrap"
    # The args file is one token per line so nothing has to be re-quoted; pair
    # each flag with its values for reading.
    awk '
        /^-/  { if (flag != "") print "  " flag; flag = $0; next }
              { if (flag != "") { flag = flag " " $0 } else print "  " $0 }
        END   { if (flag != "") print "  " flag }
    ' "$state/bwrap-args"
    echo
    echo "# host-side plan"
    sed 's/^/  /' "$state/plan"
    echo
    echo "# environment (credentials are fetched at run time and are not here)"
    sed 's/^/  /' "$state/env"
}

# What the sandbox must *not* be able to do. A mount list is a thing you can
# get wrong silently, so it is tested rather than reviewed: one stray --bind
# and this is what notices.
cmd_selftest() {
    local branch="$1"
    known_branch "$branch"
    info "running the self-test in $branch"
    sandbox_run "$branch" bash -c '
        fail=0
        check() { # description, "should_fail" command...
            local what="$1"; shift
            if "$@" >/dev/null 2>&1; then
                echo "  LEAK  $what"
                fail=1
            else
                echo "  ok    $what"
            fi
        }
        allow() {
            local what="$1"; shift
            if "$@" >/dev/null 2>&1; then echo "  ok    $what"
            else echo "  BROKE $what"; fail=1; fi
        }
        echo "denied:"
        check "~/.ssh is not readable"            ls "$HOME/.ssh"
        check "no Claude credentials"             test -f "$HOME/.claude/.credentials.json"
        check "the host checkout is not visible"  test -e "'"$REPO_DIR"'/.env"
        check "the host cache is not visible"     test -e "'"$CACHE_ROOT"'"
        check "no route off the machine"          curl -sS --max-time 5 https://example.com
        check "the proxy refuses an unlisted host" curl -sS --max-time 10 --proxy "${HTTPS_PROXY:-http://127.0.0.1:3128}" https://example.com
        check "no DNS resolver"                   getent hosts github.com
        echo "granted:"
        allow "the clone is writable"             test -w /workspace
        allow "a private PID namespace"           test "$(ps -o comm= -p 1)" != systemd
        if [ -n "${HTTPS_PROXY:-}" ]; then
            allow "an allowed host is reachable"  curl -sS --max-time 20 -o /dev/null https://static.crates.io/
        fi
        exit $fail
    '
}

cmd_doctor() {
    local ok=0
    check() {
        if eval "$2" >/dev/null 2>&1; then printf '  ok    %s\n' "$1";
        else printf '  MISS  %s — %s\n' "$1" "$3"; ok=1; fi
    }
    echo "host:"
    check "bwrap"    "command -v bwrap"  "install bubblewrap"
    check "socat"    "command -v socat"  "install socat"
    # The lib symlinks are not optional in this probe: without them execvp
    # cannot find the dynamic loader and reports ENOENT, which reads exactly
    # like "user namespaces are disabled" and is not.
    check "unprivileged user namespaces" \
        "bwrap --unshare-user --unshare-net --tmpfs / --ro-bind /usr /usr \
            --symlink usr/bin /bin --symlink usr/lib /lib --symlink usr/lib /lib64 \
            --ro-bind /etc/ld.so.cache /etc/ld.so.cache /bin/true" \
        "your kernel or distro has them disabled; the sandbox cannot be built without them"
    check "git"      "command -v git"    "install git"
    check "jq"       "command -v jq"     "install jq"
    check "curl"     "command -v curl"   "install curl"
    # The prod broker rebuilds and scrubs a snapshot with the host's sqlite3
    # rather than linking a second copy of SQLite into itself.
    check "sqlite3"  "command -v sqlite3" "needed by the prod broker to scrub a snapshot"
    check "cargo"    "command -v cargo"  "the sandbox uses the host's toolchain, read-only"
    check "claude"   "command -v claude" "the sandbox uses the host's Claude Code, read-only"
    check "playwright browsers" \
        "test -d '${XDG_CACHE_HOME:-$HOME/.cache}/ms-playwright'" \
        "npx playwright install chromium — only the browser capability needs it"
    echo "kernel:"
    # bwrap could take the controlling terminal away with --new-session, at the
    # cost of job control and Claude Code's TUI. It does not, because the
    # syscall that would justify it is off by default since Linux 6.2.
    if [ -r /proc/sys/dev/tty/legacy_tiocsti ]; then
        if [ "$(cat /proc/sys/dev/tty/legacy_tiocsti)" = "0" ]; then
            printf '  ok    TIOCSTI is disabled\n'
        else
            printf '  WARN  TIOCSTI is enabled — a sandbox could inject into your terminal;\n'
            printf '        set dev.tty.legacy_tiocsti=0\n'
            ok=1
        fi
    else
        printf '  ok    no TIOCSTI on this kernel\n'
    fi
    echo "artifacts:"
    check "anthropic broker" "test -x '$BROKER_BIN'"      "./devbox.sh build-broker"
    check "prod broker"      "test -x '$PROD_BROKER_BIN'" "./devbox.sh build-broker"
    check "egress proxy"     "test -x '$PROXY_BIN'"       "./devbox.sh build-broker"
    echo "credentials (host-side, never in a sandbox):"
    check "claude login"   "test -f '$HOME/.claude/.credentials.json'" "run 'claude' on the host once"
    check "github token"   "test -f '$(config_file github-token)' -o -f '$(config_file github-app.json)'" \
                           "put a fine-grained PAT in $(config_file github-token) (chmod 600)"
    if [ -f "$(config_file github-token)" ] && [ ! -f "$(config_file github-app.json)" ]; then
        printf '  note  a PAT does not expire, and the github capability puts it in the\n'
        printf '        sandbox. A GitHub App mints an hour-long token instead — see\n'
        printf '        sandbox/brokers/github/mint-token.sh\n'
    fi
    check "prod deploy env" "test -f '$REPO_DIR/$PROD_ENV_FILE' -o -f '$PROD_ENV_FILE'" \
                           "prod-readonly reads $PROD_ENV_FILE (see docs/deployment.md)"
    return $ok
}

usage() {
    cat <<'USAGE'
Usage: ./devbox.sh <command>

  create <branch> [--with caps] [--base ref] [--profile name]
                          clone, render the config, run first-boot
  shell <branch>          a shell in the sandbox
  claude <branch> [args]  Claude Code in the sandbox
  exec <branch> -- cmd    run one command in the sandbox
  seed <branch> [user]    build, create a dev user, seed data
  grant|revoke <branch> <capability>
  list                    sandboxes, live commands, disk, capabilities
  capabilities            what each capability grants
  selftest <branch>       prove the sandbox cannot reach what it must not
  remove <branch> [--force]
  build-broker            build the host-side brokers and the egress proxy
  config <branch>         print the exact bwrap command line and host-side plan
  doctor                  check the host is ready

A sandbox lives for as long as the command running in it. There is nothing to
start and nothing to stop.
USAGE
    exit 1
}

[ $# -ge 1 ] || usage
command="$1"; shift
case "$command" in
    create)       cmd_create "$@" ;;
    shell)        cmd_shell "${1:?branch}" ;;
    claude)       cmd_claude "$@" ;;
    exec)         b="${1:?branch}"; shift; [ "${1:-}" = "--" ] && shift; cmd_exec "$b" "$@" ;;
    seed)         cmd_seed "$@" ;;
    grant)        cmd_grant "${1:?branch}" "${2:?capability}" ;;
    revoke)       cmd_revoke "${1:?branch}" "${2:?capability}" ;;
    list)         cmd_list ;;
    capabilities) cmd_capabilities ;;
    selftest)     cmd_selftest "${1:?branch}" ;;
    remove)       cmd_remove "$@" ;;
    build-broker) cmd_build_broker ;;
    config)       cmd_config "${1:?branch}" ;;
    doctor)       cmd_doctor ;;
    *)            usage ;;
esac
