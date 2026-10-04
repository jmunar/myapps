describe "read-only prod snapshots and logs from the Odroid, via the host broker"

# Same shape as `anthropic`: a Unix socket in this sandbox's runtime directory,
# relayed to a loopback port inside the namespace, reachable by no other
# sandbox because no other sandbox has that directory mounted.
#
# The guest never learns the Odroid's address and never holds a key: the broker
# owns the SSH key and only ever fills parameters into commands it owns.
broker prod
env MYAPPS_PROD_URL "http://127.0.0.1:$PROD_PORT"
