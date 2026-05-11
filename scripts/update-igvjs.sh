#!/usr/bin/env bash
# Refresh the vendored igv.js asset. Bump IGVJS_VERSION when upgrading.
set -euo pipefail
IGVJS_VERSION="${IGVJS_VERSION:-3.0.5}"
DEST="$(dirname "$0")/../crates/igv-serve/assets/igv.esm.min.js"
curl -sSL \
  "https://cdn.jsdelivr.net/npm/igv@${IGVJS_VERSION}/dist/igv.esm.min.js" \
  -o "$DEST"
echo "wrote $DEST (igv.js ${IGVJS_VERSION}, $(wc -c <"$DEST") bytes)"
