#!/usr/bin/env bash
#
# Development in a microVM: one sandbox per branch, no credentials inside it.
#
# The host keeps ~/.claude, the GitHub token, ~/.ssh, the real .env and the real
# data/. The guest gets a clone of the repository, the Rust toolchain and Claude
# Code. What crosses the boundary is exactly what the granted capabilities say,
# and the generated .devbox/<branch>/msb-args is the whole of it.
#
# Usage: sandbox/README.md. Changing any of this: sandbox/CLAUDE.md.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SANDBOX_DIR="$REPO_DIR/sandbox"
STATE_ROOT="$REPO_DIR/.devbox"
CACHE_ROOT="${XDG_CACHE_HOME:-$HOME/.cache}/msb-devbox"
RUN_ROOT="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/msb-devbox"
CONFIG_ROOT="${XDG_CONFIG_HOME:-$HOME/.config}/msb-devbox"
LOG_ROOT="${XDG_STATE_HOME:-$HOME/.local/state}/msb-devbox"
BROKER_BIN="$SANDBOX_DIR/brokers/anthropic/target/release/msb-broker-anthropic"
# Guest-local: `devbox.sh bridge` forwards this port to the broker's socket.
BROKER_PORT="${DEVBOX_BROKER_PORT:-8787}"
BROKER_URL="http://127.0.0.1:$BROKER_PORT"

# cargo-cache-shared is in the default set: without it every branch recompiles
# the whole dependency tree. It is also the one writable surface shared between
# sandboxes — revoke it for a branch whose dependencies you have not read.
DEFAULT_CAPS="${DEVBOX_CAPS:-anthropic,github,rust-deps,node-deps,cargo-cache-shared,preview}"
DEFAULT_PROFILE="${DEVBOX_PROFILE:-default}"

# ---------------------------------------------------------------------------
# Every `msb` invocation lives here. msb is beta and its flags move; when one
# changes, this is the only block to edit.
# ---------------------------------------------------------------------------
# `create`, not `run --detach`: on msb 0.7.2 a sandbox whose image command is
# running stops answering `msb exec` altogether. See sandbox/README.md.
msb_create() { local name="$1"; shift; msb create --name "$name" --replace "$@"; }
msb_stop()   { msb stop "$1"; }
msb_rm()     { msb rm -f "$1"; }
msb_exec()   { local name="$1"; shift; msb exec -t "$name" -- "$@"; }
# Never wrap an exec in a host-side `timeout`: on msb 0.7.2, killing the client
# leaves the session stuck and every later exec against that sandbox hangs
# forever. Let msb's own --timeout end the command instead.
msb_exec_quiet() { local name="$1"; shift; msb exec --timeout 30s "$name" -- "$@"; }
msb_ping()   { msb ping -q "$1" >/dev/null 2>&1; }
msb_running() { msb list --running -q 2>/dev/null | grep -qx "$1"; }

die() { echo "devbox: $*" >&2; exit 1; }
info() { echo "devbox: $*"; }
require() { command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed"; }

sandbox_name() { echo "myapps-$1"; }
clone_dir()    { echo "$(dirname "$REPO_DIR")/myapps-$1"; }
state_dir()    { echo "$STATE_ROOT/$1"; }

check_branch() {
    # No slashes: the branch name is also a directory name and a socket name.
    [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] || die "refusing branch name '$1' (letters, digits, . _ - only)"
}

known_branch() {
    [ -d "$(state_dir "$1")" ] || die "no sandbox for '$1' (./devbox.sh list)"
}

# --- configuration ---------------------------------------------------------

render() {
    local branch="$1" state caps profile
    state="$(state_dir "$branch")"
    caps="$(cat "$state/capabilities")"
    profile="$(cat "$state/profile")"
    mkdir -p "$CACHE_ROOT/target/$branch" "$CACHE_ROOT/cargo/registry" "$CACHE_ROOT/cargo/git" \
        "$CACHE_ROOT/claude/$branch" "$RUN_ROOT"
    mkdir -p "$REPO_DIR/models"

    BRANCH="$branch" \
    CLONE="$(clone_dir "$branch")" \
    TARGET_CACHE="$CACHE_ROOT/target/$branch" \
    CARGO_CACHE="$CACHE_ROOT/cargo" \
    MODELS="$REPO_DIR/models" \
    CLAUDE_STATE="$CACHE_ROOT/claude/$branch" \
    BROKER_SOCKET="$RUN_ROOT/$branch-anthropic.sock" \
    PROD_SOCKET="$RUN_ROOT/$branch-prod.sock" \
    GITHUB_TOKEN_FILE="$CONFIG_ROOT/github-token" \
    bash "$SANDBOX_DIR/render.sh" \
        --profile "$profile" --caps "$caps" --out-dir "$state"
}

# Secrets are read here and exported only for the `msb` process, so they live in
# one short-lived environment and never in a file the guest can see.
export_secrets() {
    local state="$1" name source value
    [ -s "$state/secret-files" ] || return 0
    while IFS='=' read -r name source; do
        [ -n "$name" ] || continue
        if [ "$name" = "GITHUB_TOKEN" ] && [ -f "$CONFIG_ROOT/github-app.json" ]; then
            # An App installation token expires in an hour; a PAT does not.
            value="$("$SANDBOX_DIR/brokers/github/mint-token.sh")" \
                || die "minting a GitHub App token failed"
        else
            [ -f "$source" ] || die "capability needs $source (see sandbox/README.md)"
            value="$(tr -d '\r\n' < "$source")"
        fi
        export "$name=$value"
    done < "$state/secret-files"
}

read_cli_args() {
    MSB_ARGS=()
    local line
    while IFS= read -r line; do
        [ -n "$line" ] && MSB_ARGS+=("$line")
    done < "$1/msb-args"
}

# --- brokers ---------------------------------------------------------------

ensure_brokers() {
    local branch="$1" state broker
    state="$(state_dir "$branch")"
    [ -s "$state/brokers" ] || return 0
    while IFS= read -r broker; do
        [ -n "$broker" ] || continue
        case "$broker" in
            anthropic) start_anthropic_broker "$branch" ;;
            prod) info "the prod broker is not implemented yet — skipping" ;;
            *) die "unknown broker '$broker'" ;;
        esac
    done < "$state/brokers"
}

start_anthropic_broker() {
    local branch="$1" state pid socket
    state="$(state_dir "$branch")"
    socket="$RUN_ROOT/$branch-anthropic.sock"

    if [ -f "$state/broker.pid" ] && kill -0 "$(cat "$state/broker.pid")" 2>/dev/null; then
        return 0
    fi
    [ -x "$BROKER_BIN" ] || die "broker not built — run ./devbox.sh build-broker"

    mkdir -p "$RUN_ROOT" "$LOG_ROOT"
    # setsid so closing the terminal does not take the broker — and with it
    # every Claude Code session in the sandbox — down with it.
    setsid "$BROKER_BIN" \
        --socket "$socket" \
        --sandbox "$branch" \
        --audit-log "$LOG_ROOT/audit.jsonl" \
        ${DEVBOX_BROKER_ARGS:-} \
        >>"$LOG_ROOT/$branch-broker.log" 2>&1 &
    pid=$!
    echo "$pid" > "$state/broker.pid"
    sleep 0.3
    kill -0 "$pid" 2>/dev/null || die "broker died on start — see $LOG_ROOT/$branch-broker.log"
    info "broker running on $socket (pid $pid)"
}

stop_broker() {
    local state="$1"
    [ -f "$state/broker.pid" ] || return 0
    kill "$(cat "$state/broker.pid")" 2>/dev/null || true
    rm -f "$state/broker.pid"
}

# Guest scripts run from the mounted clone rather than over `msb exec` stdin:
# whether exec forwards stdin is a property of msb we would rather not depend on,
# and the clone is already there. The copy is removed afterwards so it never
# shows up as an uncommitted change.
run_guest_script() {
    local branch="$1" script="$2"; shift 2
    local clone name target
    clone="$(clone_dir "$branch")"
    name="$(sandbox_name "$branch")"
    target=".devbox-$(basename "$script")"
    cp "$script" "$clone/$target"
    chmod +x "$clone/$target"
    local status=0
    msb_exec "$name" bash "/workspace/$target" "$@" || status=$?
    rm -f "$clone/$target"
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
    require git; require msb

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
    # --no-hardlinks is not hygiene: with hardlinks the clone's objects are the
    # same inodes as this repository's, and a guest that rewrites one corrupts
    # the host's copy.
    git clone --no-hardlinks --branch "$base" "$REPO_DIR" "$clone"
    git -C "$clone" checkout -b "$branch"

    # Point at GitHub over HTTPS: header substitution only works on HTTP, and
    # there is no SSH key in the VM by design.
    local origin
    origin="$(git -C "$REPO_DIR" remote get-url origin)"
    origin="${origin/git@github.com:/https://github.com/}"
    git -C "$clone" remote set-url origin "$origin"
    echo ".devbox-*" >> "$clone/.git/info/exclude"
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
    ensure_brokers "$branch"

    read_cli_args "$state"
    export_secrets "$state"
    msb_create "$(sandbox_name "$branch")" "${MSB_ARGS[@]}"

    info "running first-boot"
    run_guest_script "$branch" "$SANDBOX_DIR/bootstrap/first-boot.sh"
    cmd_up "$branch"

    echo
    info "sandbox ready:  ./devbox.sh claude $branch"
    info "capabilities:   $caps"
    info "config:         ./devbox.sh config $branch"
}

cmd_up() {
    local branch="$1"
    known_branch "$branch"
    local name state
    name="$(sandbox_name "$branch")"
    state="$(state_dir "$branch")"

    render "$branch"
    ensure_brokers "$branch"
    if ! msb_running "$name"; then
        read_cli_args "$state"
        export_secrets "$state"
        msb_create "$name" "${MSB_ARGS[@]}"
    fi

    local attempt
    for attempt in 1 2 3 4 5 6 7 8 9 10; do
        msb_ping "$name" && break
        sleep 1
    done
    # Nothing to set up for TLS: msb installs its interception CA in the guest
    # and points NODE_EXTRA_CA_CERTS, SSL_CERT_FILE and CURL_CA_BUNDLE at it.
    #
    # A broker the guest cannot reach looks exactly like a Claude Code auth
    # problem from inside the sandbox, so check rather than hope. The bridge is
    # the sandbox's image command, so give it a moment after a cold start.
    # One attempt, never retried: each aborted exec costs the sandbox its exec
    # channel, so a failed check must not turn into four more.
    if grep -qx anthropic "$state/brokers" 2>/dev/null; then
        msb_exec_quiet "$name" curl -sf --max-time 5 -o /dev/null \
            "$BROKER_URL/_broker/health" >/dev/null 2>&1 \
            || info "warning: broker not reachable from the sandbox"
    fi
}

cmd_stop() {
    local branch="$1"
    known_branch "$branch"
    msb_stop "$(sandbox_name "$branch")" || true
    stop_broker "$(state_dir "$branch")"
    info "stopped $branch"
}

cmd_shell()  { known_branch "$1"; cmd_up "$1" >/dev/null; msb_exec "$(sandbox_name "$1")" bash -l; }
cmd_claude() { local b="$1"; shift; known_branch "$b"; cmd_up "$b" >/dev/null; msb_exec "$(sandbox_name "$b")" claude "$@"; }
cmd_exec()   { local b="$1"; shift; known_branch "$b"; cmd_up "$b" >/dev/null; msb_exec "$(sandbox_name "$b")" "$@"; }

cmd_seed() {
    local branch="$1"; shift
    known_branch "$branch"
    cmd_up "$branch" >/dev/null
    run_guest_script "$branch" "$SANDBOX_DIR/bootstrap/seed.sh" "$@"
}

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
        render "$branch"
        die "did not grant $cap"
    fi
    info "granted $cap — restarting the sandbox to apply it"
    cmd_stop "$branch" >/dev/null
    cmd_up "$branch"
}

cmd_revoke() {
    local branch="$1" cap="$2"
    known_branch "$branch"
    local state caps
    state="$(state_dir "$branch")"
    local kept="" item existing
    IFS=',' read -r -a existing <<< "$(cat "$state/capabilities")"
    for item in "${existing[@]}"; do
        { [ -z "$item" ] || [ "$item" = "$cap" ]; } && continue
        kept="${kept:+$kept,}$item"
    done
    caps="$kept"
    echo "$caps" > "$state/capabilities"
    render "$branch"
    info "revoked $cap — restarting the sandbox to apply it"
    cmd_stop "$branch" >/dev/null
    cmd_up "$branch"
}

cmd_list() {
    [ -d "$STATE_ROOT" ] || { info "no sandboxes"; return 0; }
    printf '%-24s %-10s %-8s %s\n' BRANCH STATE SIZE CAPABILITIES
    local dir branch state size
    for dir in "$STATE_ROOT"/*/; do
        [ -d "$dir" ] || continue
        branch="$(basename "$dir")"
        if msb_running "$(sandbox_name "$branch")"; then state=running; else state=stopped; fi
        size="$(du -sh "$CACHE_ROOT/target/$branch" 2>/dev/null | cut -f1)"
        printf '%-24s %-10s %-8s %s\n' "$branch" "$state" "${size:--}" "$(cat "$dir/capabilities")"
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

    msb_rm "$(sandbox_name "$branch")" 2>/dev/null || true
    stop_broker "$state"
    rm -f "$RUN_ROOT/$branch-anthropic.sock" "$RUN_ROOT/$branch-prod.sock"
    rm -rf "$clone" "$state" "$CACHE_ROOT/target/$branch" "$CACHE_ROOT/claude/$branch"

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
    (cd "$SANDBOX_DIR/brokers/anthropic" && cargo build --release)
    info "built $BROKER_BIN"
}

cmd_build_image() {
    require docker; require msb
    docker build -t myapps-dev:latest "$SANDBOX_DIR/image"
    # msb keeps its own image cache; a tag in docker's is invisible to it.
    docker save myapps-dev:latest | msb load -t myapps-dev:latest
    if [ "${1:-}" = "--browser" ]; then
        docker build -t myapps-dev-browser:latest \
            -f "$SANDBOX_DIR/image/Dockerfile.browser" "$SANDBOX_DIR/image"
        docker save myapps-dev-browser:latest | msb load -t myapps-dev-browser:latest
    fi
    info "loaded into msb: $(msb image list 2>/dev/null | grep -c myapps-dev) image(s)"
}

# The bridge cannot be started in the background: on msb 0.7.2 a persistent
# guest process started as the image command, or through an exec, stops the
# sandbox answering further execs — and killing the exec client poisons it too.
# So it runs in the foreground, in its own terminal, until the broker moves to
# TCP. See "Blocked" in sandbox/README.md.
cmd_bridge() {
    local branch="$1"
    known_branch "$branch"
    echo "devbox: bridging 127.0.0.1:$BROKER_PORT in the sandbox to the host broker."
    echo "devbox: leave this running in its own terminal. Ctrl-C wedges the"
    echo "devbox: sandbox's exec channel — use './devbox.sh remove' if you do."
    msb exec "$(sandbox_name "$branch")" -- \
        socat "TCP-LISTEN:$BROKER_PORT,fork,reuseaddr,bind=127.0.0.1" \
        "VSOCK-CONNECT:2:5000"
}

cmd_config() {
    local branch="$1"
    known_branch "$branch"
    render "$branch"
    echo "msb create --name $(sandbox_name "$branch") \\"
    # The args file is one token per line so nothing has to be re-quoted;
    # pair each flag with its value for reading.
    awk '''
        /^-/  { if (flag != "") print "  " flag; flag = $0; next }
              { if (flag != "") { print "  " flag " " $0; flag = "" }
                else print "  " $0 }
        END   { if (flag != "") print "  " flag }
    ''' "$(state_dir "$branch")/msb-args"
}

cmd_doctor() {
    local ok=0
    check() {
        if eval "$2" >/dev/null 2>&1; then printf '  ok    %s\n' "$1";
        else printf '  MISS  %s — %s\n' "$1" "$3"; ok=1; fi
    }
    echo "host:"
    check "msb"            "command -v msb"            "install microsandbox: https://docs.microsandbox.dev"
    check "KVM"            "test -r /dev/kvm -a -w /dev/kvm" "add yourself to the kvm group"
    check "git"            "command -v git"            "install git"
    check "jq"             "command -v jq"             "install jq"
    check "docker"         "command -v docker"         "needed only to build the guest image"
    echo "artifacts:"
    check "broker binary"  "test -x '$BROKER_BIN'"     "./devbox.sh build-broker"
    check "guest image"    "msb image list | grep -q myapps-dev" "./devbox.sh build-image"
    echo "credentials (host-side, never in a sandbox):"
    check "claude login"   "test -f '$HOME/.claude/.credentials.json'" "run 'claude' on the host once"
    check "github token"   "test -f '$CONFIG_ROOT/github-token' -o -f '$CONFIG_ROOT/github-app.json'" \
                           "put a fine-grained PAT in $CONFIG_ROOT/github-token (chmod 600)"
    local dir branch
    if [ -d "$STATE_ROOT" ]; then
        echo "brokers:"
        for dir in "$STATE_ROOT"/*/; do
            [ -d "$dir" ] || continue
            branch="$(basename "$dir")"
            if [ -S "$RUN_ROOT/$branch-anthropic.sock" ] && command -v curl >/dev/null; then
                printf '  %-20s %s\n' "$branch" \
                    "$(curl -s --unix-socket "$RUN_ROOT/$branch-anthropic.sock" \
                        http://localhost/_broker/health || echo unreachable)"
            fi
        done
    fi
    return $ok
}

usage() {
    cat <<'USAGE'
Usage: ./devbox.sh <command>

  create <branch> [--with caps] [--base ref] [--profile name]
                          clone, render the config, boot the sandbox
  up <branch>             start the sandbox and its brokers
  stop <branch>           stop both
  shell <branch>          a shell in the sandbox
  claude <branch> [args]  Claude Code in the sandbox
  exec <branch> -- cmd    run one command in the sandbox
  seed <branch> [user]    build, create a dev user, seed data
  grant|revoke <branch> <capability>
  list                    sandboxes, state, disk, capabilities
  capabilities            what each capability grants
  remove <branch> [--force]
  build-broker            build the host-side Anthropic broker
  build-image [--browser] build the guest image(s) and load them into msb
  config <branch>         print the exact msb command line for a sandbox
  bridge <branch>         forward the sandbox to the host broker (foreground)
  doctor                  check the host is ready
USAGE
    exit 1
}

[ $# -ge 1 ] || usage
command="$1"; shift
case "$command" in
    create)       cmd_create "$@" ;;
    up)           cmd_up "${1:?branch}" ;;
    stop)         cmd_stop "${1:?branch}" ;;
    shell)        cmd_shell "${1:?branch}" ;;
    claude)       cmd_claude "$@" ;;
    exec)         b="${1:?branch}"; shift; [ "${1:-}" = "--" ] && shift; cmd_exec "$b" "$@" ;;
    seed)         cmd_seed "$@" ;;
    grant)        cmd_grant "${1:?branch}" "${2:?capability}" ;;
    revoke)       cmd_revoke "${1:?branch}" "${2:?capability}" ;;
    list)         cmd_list ;;
    capabilities) cmd_capabilities ;;
    remove)       cmd_remove "$@" ;;
    build-broker) cmd_build_broker ;;
    build-image)  cmd_build_image "$@" ;;
    config)       cmd_config "${1:?branch}" ;;
    bridge)       cmd_bridge "${1:?branch}" ;;
    doctor)       cmd_doctor ;;
    *)            usage ;;
esac
