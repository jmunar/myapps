describe "git push and gh pr create, with a token that expires in an hour"

# Unlike every other credential here, this one is *in* the sandbox. Injecting
# it from outside needs TLS interception, and there is none: the egress proxy
# tunnels bytes it cannot read. What replaces that is the token's own shape —
# `devbox.sh` mints a GitHub App installation token per sandbox start, scoped
# to this repository and expiring in an hour, which is close to exactly what
# the capability grants anyway. A fine-grained PAT works and does not expire,
# and `doctor` says so.
credential GH_TOKEN github

allow github.com api.github.com codeload.github.com objects.githubusercontent.com

env GIT_ASKPASS /opt/devbox/bin/devbox-git-askpass
