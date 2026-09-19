describe "Claude Code reaches the model through the host broker"

# The broker listens on a host loopback port. `allow host:...` is msb's `host`
# network group — the machine msb runs on, reached at $HOST_ALIAS — and the
# rule is scoped to the one port, so this grants nothing else on the host.
#
# Loopback is not authorisation: every process on the host, and every other
# sandbox granted `host`, can open that port too. So the broker only answers
# requests carrying this sandbox's token, generated on the host at `create`.
# The guest does hold the real value, unlike GITHUB_TOKEN — it has to, since
# there is no TLS interception on a plain-HTTP host port to substitute it. What
# it buys is model access on the subscription and nothing else: the broker
# substitutes the OAuth credential on the way out and never returns it.
broker anthropic
allow "host:tcp:$BROKER_PORT"

env ANTHROPIC_BASE_URL "http://$HOST_ALIAS:$BROKER_PORT"
env ANTHROPIC_AUTH_TOKEN "$BROKER_TOKEN"
env CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC 1
# MCP tool search is off by default against a non-first-party base URL. The
# broker forwards tool_reference blocks untouched, so re-enable it.
env ENABLE_TOOL_SEARCH true
