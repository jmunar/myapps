describe "Claude Code reaches the model through the host broker"

# The broker listens on a Unix socket in this sandbox's runtime directory, and
# a socat inside the namespace relays 127.0.0.1:$ANTHROPIC_PORT to it. There is
# no token: a sandbox that was not given that directory cannot name the socket,
# and nothing else on the host is looking for it.
#
# The port is a constant rather than something claimed, because the sandbox has
# its own network namespace — two sandboxes both using 8080 never meet.
broker anthropic

env ANTHROPIC_BASE_URL "http://127.0.0.1:$ANTHROPIC_PORT"
# Claude Code will start an OAuth flow if it has no token at all, so it gets
# one. Its value is not a secret and not checked: the broker strips whatever
# the client sent and substitutes the real credential on the way out.
env ANTHROPIC_AUTH_TOKEN devbox
env CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC 1
# MCP tool search is off by default against a non-first-party base URL. The
# broker forwards tool_reference blocks untouched, so re-enable it.
env ENABLE_TOOL_SEARCH true
