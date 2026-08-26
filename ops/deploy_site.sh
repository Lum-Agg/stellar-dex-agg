#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/packages/frontend"
# Prefer committed production Limit env if present; else .env.production.local
if [[ -f .env.production.local ]]; then
  set -a
  # shellcheck disable=SC1091
  source .env.production.local
  set +a
fi
npm run build
# npx wrangler pages project create lumagg
npx wrangler pages deploy out --project-name=lumagg --commit-dirty=true
