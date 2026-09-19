describe "Claude Code reaches the model through the host broker"

# The broker is a host Unix socket routed to a guest vsock port, so this grants
# no network rule. The guest side of that route has to be bridged to a TCP port
# by hand for now — see "Blocked" in sandbox/README.md, and `devbox.sh bridge`.
broker anthropic
cli --vsock "$BROKER_SOCKET:5000"

env ANTHROPIC_BASE_URL http://127.0.0.1:8787
# The broker strips this and substitutes the real credential on the host.
env ANTHROPIC_AUTH_TOKEN sandbox-placeholder
env CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC 1
# MCP tool search is off by default against a non-first-party base URL. The
# broker forwards tool_reference blocks untouched, so re-enable it.
env ENABLE_TOOL_SEARCH true
