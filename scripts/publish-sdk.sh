#!/usr/bin/env bash
# Publish @lumagg/sdk to npm (Tranche 2).
#
# Prereqs: npm login, version bump in packages/sdk/package.json
#
# Usage:
#   ./scripts/publish-sdk.sh          # dry-run (pack only)
#   ./scripts/publish-sdk.sh --publish
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="$ROOT/packages/sdk"
PUBLISH=false
[[ "${1:-}" == "--publish" ]] && PUBLISH=true

cd "$SDK"
npm install
npm run build
npm pack --dry-run 2>&1 | tail -15

if $PUBLISH; then
  echo "=== Publishing to npm ==="
  npm publish --access public
  echo "Done. Tag repo: git tag sdk-v$(node -p "require('./package.json').version")"
else
  echo
  echo "Dry-run only. To publish: ./scripts/publish-sdk.sh --publish"
fi
