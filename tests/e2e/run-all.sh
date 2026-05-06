#!/usr/bin/env bash
# Runs every E2E spec sequentially. Each spec is self-contained — its own
# tempdir, its own daemon — so order doesn't matter.
#
# Pre-reqs:
#   - Vite dev server on :5179 (`bun --cwd frontend run dev`)
#   - shmark-desktop binary built (`cargo build -p shmark-tauri`)

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

specs=(
  tests/e2e/share-from-clipboard.spec.ts
  tests/e2e/copy-share-code.spec.ts
  tests/e2e/folder-share.spec.ts
  tests/e2e/settings.spec.ts
)

failed=0
for spec in "${specs[@]}"; do
  echo
  echo "========================================="
  echo "  $spec"
  echo "========================================="
  if ! bun "$spec"; then
    failed=1
  fi
done

if [ "$failed" = 1 ]; then
  echo
  echo "✗ one or more E2E specs failed"
  exit 1
fi

echo
echo "✓ all E2E specs passed"
