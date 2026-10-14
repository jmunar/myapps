#!/usr/bin/env bash
# Compare two solvers on the same UGPhysics problems, unattended: each phase
# starts its own llama-server, runs, and stops it before the next, so only one
# model is ever in memory. Every stage resumes, so after a crash or Ctrl-C just
# run this again. See README.md, "Checking the grades against a solver".
#
#   crates/myapps-challenges-prep/solve-compare.sh            # 40 problems
#   LIMIT=60 crates/myapps-challenges-prep/solve-compare.sh
#
# Logs go to ~/solve-compare.log (stages) and ~/solve-compare-server.log.
set -euo pipefail

# Keep the laptop awake for the whole run.
if [[ -z ${INHIBITED:-} ]] && command -v systemd-inhibit >/dev/null; then
    INHIBITED=1 exec systemd-inhibit --what=sleep:idle --why="solve-compare" "$0" "$@"
fi

LIMIT=${LIMIT:-40}
DATASET=${DATASET:-ugphysics}
MODELS=${MODELS:-$HOME/Models}
PORT=${PORT:-8091}
QWEN=Qwen3.6-35B-A3B-UD-Q4_K_M.gguf
GEMMA=gemma-4-26B-A4B-it-UD-Q4_K_M.gguf
LOG=${LOG:-$HOME/solve-compare.log}
SERVER_LOG=${SERVER_LOG:-$HOME/solve-compare-server.log}

# Must match solve::server_options (thinking budget and context).
SOLVE_OPTS=(-c 16384 --reasoning-budget 8192
    --reasoning-budget-message "Time is up, so I'll give my final answer now.")
CHECK_OPTS=(-c 16384)

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
PREP=${PREP:-$ROOT/target/release/myapps-challenges-prep}
SERVER_PID=

log() { echo "$(date '+%F %T') $*" | tee -a "$LOG"; }

stop_server() {
    if [[ -n $SERVER_PID ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
        log "stopping llama-server ($SERVER_PID)"
        kill "$SERVER_PID"
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    SERVER_PID=
}
trap stop_server EXIT
trap 'log "interrupted"; exit 130' INT TERM

start_server() {
    local model=$1
    shift
    stop_server
    log "starting llama-server with $model"
    llama-server -m "$MODELS/$model" -ngl 99 --parallel 1 --port "$PORT" "$@" \
        >>"$SERVER_LOG" 2>&1 &
    SERVER_PID=$!
    # Loading 16–21 GiB of weights takes a while; /health answers 200 when ready.
    for _ in $(seq 600); do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            log "llama-server exited while loading; see $SERVER_LOG"
            exit 1
        fi
        if curl -sf "http://127.0.0.1:$PORT/health" >/dev/null; then
            log "llama-server ready"
            return
        fi
        sleep 2
    done
    log "llama-server not ready after 20 minutes"
    exit 1
}

prep() {
    log "grade $*"
    "$PREP" grade "$@" 2>&1 | tee -a "$LOG"
}

if curl -s "http://127.0.0.1:$PORT/health" >/dev/null 2>&1; then
    echo "Something already answers on port $PORT; stop it first (one model at a time)." >&2
    exit 1
fi
for m in "$QWEN" "$GEMMA"; do
    [[ -f $MODELS/$m ]] || { echo "Missing $MODELS/$m" >&2; exit 1; }
done

log "building the prep tool"
cargo build --release -q -p myapps-challenges-prep --manifest-path "$ROOT/Cargo.toml"

run() {
    start_server "$QWEN" "${SOLVE_OPTS[@]}"
    prep solve "$DATASET" --limit "$LIMIT"

    start_server "$GEMMA" "${SOLVE_OPTS[@]}"
    prep solve "$DATASET" --limit "$LIMIT" --solver "$GEMMA"

    # Qwen3.6 re-checks Gemma's answers, so both share a checker.
    start_server "$QWEN" "${CHECK_OPTS[@]}"
    prep check "$DATASET" --solver "$GEMMA"
    stop_server

    log "=== Qwen3.6 ==="
    prep rank "$DATASET" --limit 300
    log "=== Gemma 4 ==="
    prep rank "$DATASET" --limit 300 --solver "$GEMMA"
    log "done"
}

run
