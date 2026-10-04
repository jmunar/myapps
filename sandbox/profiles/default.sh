describe "Base sandbox — no credentials, no host paths, nothing reachable"

# Enforced by a systemd scope when the host has one; see devbox.sh. Without
# systemd they are advisory, and the sandbox is bounded by the host instead.
cpus 4
memory 8G

workdir /workspace
hostname "$BRANCH"

env RUST_BACKTRACE 1
# Claude Code's non-essential endpoints are not in any allow list, so leaving
# these on only produces refused CONNECTs and log noise.
env DISABLE_TELEMETRY 1
env DISABLE_ERROR_REPORTING 1
env DISABLE_AUTOUPDATER 1

# The clone, and the only host directory the sandbox writes by default.
mount "$CLONE:/workspace"
mount "$TARGET_CACHE:/workspace/target"
mount "$MODELS:/workspace/models:ro"
# Per branch unless `cargo-cache-shared` is granted, which mounts the shared
# registry *inside* this one — which is why the parent has to be declared here,
# in the profile, ahead of any capability.
mount "$CARGO_HOME_DIR:$HOME_DIR/.cargo"
# $HOME is a tmpfs, so the agent's own history and settings only survive
# because they live on this mount.
mount "$CLAUDE_STATE:$HOME_DIR/.claude"
