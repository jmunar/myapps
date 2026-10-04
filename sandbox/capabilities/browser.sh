describe "Chromium and Playwright, for /frontend-walkthrough"

# The host's own Playwright browsers, read-only — there is no guest image to
# bake them into any more. `npx playwright install chromium` on the host is
# what populates this; `doctor` says so when it is missing.
mount "$PLAYWRIGHT_CACHE:$HOME_DIR/.cache/ms-playwright:ro"
env PLAYWRIGHT_BROWSERS_PATH "$HOME_DIR/.cache/ms-playwright"

# Chromium wants a real /dev/shm and the host's font configuration; without
# the first it crashes on start, without the second every screenshot renders
# in a fallback face.
cli --tmpfs /dev/shm
cli --ro-bind-try /etc/fonts /etc/fonts

allow cdn.playwright.dev
