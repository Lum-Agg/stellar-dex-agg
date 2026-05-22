#!/bin/bash
# One-shot server setup: RPC-owned shared stellar-core (config only).
# Delegates to configure_shared_core.sh (default mode=owned).
#
# Usage (on server as root): ./deploy/setup_shared_stellar_core.sh
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SHARED_CORE_MODE="${SHARED_CORE_MODE:-owned}"
exec "${SCRIPT_DIR}/configure_shared_core.sh"
