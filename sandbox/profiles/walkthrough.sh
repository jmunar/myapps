describe "Base sandbox on the browser image, for /frontend-walkthrough"

image myapps-dev-browser:latest
cpus 4
memory 8G
workdir /workspace
user dev
hostname "$BRANCH"

env CARGO_HOME /home/dev/.cargo
env RUST_BACKTRACE 1
env PLAYWRIGHT_BROWSERS_PATH /opt/playwright
env DISABLE_TELEMETRY 1
env DISABLE_ERROR_REPORTING 1
env DISABLE_AUTOUPDATER 1

# Quotas, because msb defaults every mount to 4096 MiB. /workspace holds the
# clone and the broker's own target/, which the overmount below does not
# cover; /workspace/target holds a full debug build plus clippy and test
# artifacts, which went past 6 GiB the first time anyone measured it.
mount "$CLONE:/workspace:quota=16384"
mount "$TARGET_CACHE:/workspace/target:quota=32768"
mount "$MODELS:/workspace/models:ro"
# `up` recreates the sandbox, so the guest disk is ephemeral: the agent's
# own history and settings only survive if they live on a mount.
mount "$CLAUDE_STATE:/home/dev/.claude"

deny 169.254.169.254
