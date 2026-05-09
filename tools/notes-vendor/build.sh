#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -d node_modules ]; then
  npm install
fi

OUT="../../static/notes-vendor.bundle.js"

npx esbuild entry.js \
  --bundle \
  --format=iife \
  --global-name=NotesYjsTiptap \
  --minify \
  --target=es2022 \
  --outfile="$OUT"

echo "Built $OUT"
ls -lh "$OUT"
