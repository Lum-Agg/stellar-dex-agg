#!/usr/bin/env bash
# Compare LumAgg quotes vs Soroswap API (optional) for SCF differentiation evidence.
#
# Usage:
#   ./scripts/scf-benchmark.sh
#   SOROSWAP_API_KEY=sk_... ./scripts/scf-benchmark.sh
#   OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh
#
# Env:
#   LUMAGG_API          default https://api.lumagg.xyz
#   SOROSWAP_API_URL    default https://api.soroswap.finance
#   SOROSWAP_API_KEY    optional (register at https://api.soroswap.finance/register)
#   OUTPUT              if set, write markdown to this path (also printed to stdout)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export LUMAGG_API="${LUMAGG_API:-https://api.lumagg.xyz}"
export SOROSWAP_API_URL="${SOROSWAP_API_URL:-https://api.soroswap.finance}"
export SOROSWAP_API_KEY="${SOROSWAP_API_KEY:-}"
export OUTPUT="${OUTPUT:-}"

python3 "$ROOT/scripts/scf_benchmark.py"
