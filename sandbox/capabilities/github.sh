describe "git push and gh pr create; the token value never enters the VM"

secret GITHUB_TOKEN github.com api.github.com
secret_file GITHUB_TOKEN "$GITHUB_TOKEN_FILE"

allow github.com api.github.com codeload.github.com objects.githubusercontent.com

# Both are the placeholder, not the token: msb substitutes the real value
# outside the VM and only for the hosts above.
env GH_TOKEN '$MSB_GITHUB_TOKEN'
env GIT_ASKPASS /usr/local/bin/devbox-git-askpass
