# Idea 01: Cross-compile in GitHub Actions instead of on-device

## Summary

Replace the current on-device build (rsync source → build on Odroid N2) with
cross-compilation in GitHub Actions, deploying a pre-built aarch64 binary.

## Pros

- **Faster builds** — CI runners have more CPU/RAM than the Odroid N2.
- **No build load on device** — Odroid stays free to serve traffic; no OOM or
  CPU starvation risk during builds.
- **Simpler deploy** — Ship a single binary instead of the full source tree.
  Deploy becomes "copy binary + restart service."
- **Reproducible** — Build environment is identical every run; no drift from
  packages installed on the Odroid over time.
- **Scales** — Adding more deploy targets doesn't require build toolchains on
  each one.

## Cons

- **Native dependency cross-linking** — SQLite is fine if using the `bundled`
  feature. OpenSSL would need an aarch64 sysroot or a switch to `rustls`.
- **Cross-compile toolchain setup** — Requires `gcc-aarch64-linux-gnu`, the
  `aarch64-unknown-linux-gnu` Rust target, and linker config in
  `.cargo/config.toml`. Adds CI complexity.
- **Binary compatibility / glibc mismatch** — The linked glibc must be ≤ the
  version on Ubuntu 24.04 (Odroid). Mitigations: pin the sysroot, or target
  `aarch64-unknown-linux-musl` for a fully static binary (minor
  performance/compatibility trade-offs with musl).
- **Harder to debug build failures** — Cross-compile env is harder to reproduce
  locally (Docker with the same image helps).
- **CI minutes** — Rust release builds are heavy; eats into GitHub Actions
  free-tier minutes.
- **Artifact transfer** — Need to SCP the binary to the Odroid rather than
  building in-place.

## Recommended approach

1. Use `cross` (Rust cross-compilation tool) or manually set up the
   `aarch64-unknown-linux-gnu` target in CI.
2. Use the `bundled` feature for SQLite to avoid cross-sysroot issues.
3. Consider switching to `rustls` instead of OpenSSL to eliminate the biggest
   cross-compile headache.
4. Deploy step simplifies to: `scp binary → restart service`.
5. For zero glibc worry, target `aarch64-unknown-linux-musl` (fully static).

## Next steps

- [ ] Audit current dependencies for cross-compile friendliness (OpenSSL vs
      rustls, SQLite bundled feature, etc.).
- [ ] Prototype the CI workflow in a branch.
- [ ] Benchmark build time: CI cross-compile vs. on-device.
- [ ] Decide static (musl) vs. dynamic (gnu) linking.
