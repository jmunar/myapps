describe "share the crate cache with every other sandbox (writable, shared)"

# The one writable surface shared between sandboxes. It saves re-downloading and
# re-compiling the dependency tree per branch, at the cost of a path by which a
# hostile build script in one sandbox can plant sources another sandbox
# compiles. Revoke it for a branch whose dependencies you have not read.
mount "$CARGO_CACHE/registry:/home/dev/.cargo/registry"
mount "$CARGO_CACHE/git:/home/dev/.cargo/git"
