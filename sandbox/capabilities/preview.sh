describe "reach the dev server at 127.0.0.1:3000 on the host"

port 3000:3000
# The guest must listen on all interfaces or the published port reaches
# nothing — 127.0.0.1 inside the VM is the VM's own loopback.
env BIND_ADDR 0.0.0.0:3000
