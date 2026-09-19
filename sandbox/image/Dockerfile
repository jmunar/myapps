# The guest image: a Rust development environment and Claude Code, and no
# credentials of any kind. Everything secret is reached through a host broker
# or an msb secret, so nothing here needs baking in.
FROM debian:bookworm-slim

ARG RUST_VERSION=1.98.1
ARG NODE_MAJOR=22
ARG GH_VERSION=2.62.0
# Pin Claude Code: an image that changes under you is not a boundary you can
# reason about. Bump deliberately.
ARG CLAUDE_CODE_VERSION=latest

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/home/dev/.cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/home/dev/.cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev \
        ca-certificates curl git jq sqlite3 socat openssl xz-utils \
        procps less vim-tiny bash-completion iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Node, for Claude Code.
RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# gh, for `gh pr create` from inside the sandbox.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    curl -fsSL "https://github.com/cli/cli/releases/download/v${GH_VERSION}/gh_${GH_VERSION}_linux_${arch}.tar.gz" \
        | tar -xz -C /tmp; \
    install -m 0755 "/tmp/gh_${GH_VERSION}_linux_${arch}/bin/gh" /usr/local/bin/gh; \
    rm -rf /tmp/gh_*

# uid 1000 matches the host user, so bind-mounted files keep their ownership.
RUN useradd --create-home --uid 1000 --shell /bin/bash dev

RUN curl -fsSL https://sh.rustup.rs | \
        sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "${RUST_VERSION}" --component rustfmt --component clippy \
    && chmod -R a+w "${RUSTUP_HOME}" \
    && chown -R dev:dev /home/dev/.cargo

RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && npm cache clean --force

COPY guest/ /usr/local/bin/
RUN chmod 0755 /usr/local/bin/devbox-*

USER dev
WORKDIR /workspace

# cargo-audit backs the PostToolUse hook; without it the hook is silently inert.
RUN cargo install cargo-audit --locked && rm -rf /home/dev/.cargo/registry

# A login shell sources /etc/profile, which resets PATH and loses the ENV above
# — so `devbox.sh shell` would open a sandbox with no cargo in it.
USER root
RUN printf 'export PATH=/home/dev/.cargo/bin:$PATH\n' > /etc/profile.d/devbox-path.sh
USER dev

CMD ["/bin/bash"]
