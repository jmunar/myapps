describe "reach the dev server from any device on the LAN (phone review)"

# Publishes beyond loopback. Grant it for a phone pass, revoke it after.
conflicts preview

port 0.0.0.0:3000:3000
env BIND_ADDR 0.0.0.0:3000
