describe "reach the dev server at 127.0.0.1:3000 on the host"

# The sandbox has no network beyond its own loopback, so the dev server is
# published by a pair of socats over a Unix socket rather than by a route: the
# host listens on 127.0.0.1:3000 and hands each connection to the sandbox.
port 127.0.0.1:3000:3000
env BIND_ADDR 127.0.0.1:3000
