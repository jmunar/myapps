describe "read-only prod snapshots and logs from the Odroid, via the host broker"

# Same shape as `anthropic`: a host loopback port, reachable only because this
# asks for msb's `host` group and only on that one port, and answered only for
# the sandbox's own token.
#
# The guest never learns the Odroid's address and never holds a key: the broker
# owns the SSH key and only ever fills parameters into commands it owns.
broker prod
allow "host:tcp:$PROD_PORT"
env MYAPPS_PROD_URL "http://$HOST_ALIAS:$PROD_PORT"
env MYAPPS_PROD_TOKEN "$BROKER_TOKEN"
