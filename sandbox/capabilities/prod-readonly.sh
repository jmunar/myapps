describe "read-only prod snapshots and logs from the Odroid, via the host broker"

# The guest never learns the Odroid's address and never holds a key: the broker
# owns the SSH key and only ever fills parameters into commands it owns.
broker prod
cli --vsock "$PROD_SOCKET:5001"
env MYAPPS_PROD_URL http://127.0.0.1:8788
