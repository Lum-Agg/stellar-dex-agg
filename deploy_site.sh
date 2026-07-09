#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/packages/frontend"
npm run build
# npx wrangler pages project create lumagg
npx wrangler pages deploy out --project-name=lumagg --commit-dirty=true
