describe "share the crate cache with every other sandbox (writable, shared)"

# The one writable surface shared between sandboxes. It saves re-downloading
# and re-compiling the dependency tree per branch, at the cost of a path by
# which a hostile build script in one sandbox can plant sources another sandbox
# compiles. Revoke it for a branch whose dependencies you have not read.
#
# These nest inside the profile's $HOME/.cargo mount, so they have to be
# rendered after it — which they are, because capabilities apply after the
# profile and bwrap performs mounts in the order given.
mount "$CARGO_CACHE/registry:$HOME_DIR/.cargo/registry"
mount "$CARGO_CACHE/git:$HOME_DIR/.cargo/git"
