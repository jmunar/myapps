describe "share the crate cache with every other sandbox (writable, shared)"

# The one writable surface shared between sandboxes. It saves re-downloading and
# re-compiling the dependency tree per branch, at the cost of a path by which a
# hostile build script in one sandbox can plant sources another sandbox
# compiles. Revoke it for a branch whose dependencies you have not read.
# Shared by every branch and append-only in practice, so it outgrows msb's
# 4096 MiB default mount quota sooner than anything else here.
mount "$CARGO_CACHE/registry:/home/dev/.cargo/registry:quota=16384"
mount "$CARGO_CACHE/git:/home/dev/.cargo/git:quota=8192"
