#!/usr/bin/env bash
# Vendor acme.sh's core script + the dnsapi plugins the panel's DNS-01 provider
# hook uses. The panel does NOT run acme.sh as an ACME client — it only sources
# the core (for helper functions like _get/_post) and a provider plugin, then
# calls dns_<plugin>_add/_rm to set the _acme-challenge TXT record. acme.sh is
# GPLv3; we redistribute it unmodified (invoked as separate scripts).
#
# Keep the plugin list in sync with backend/src/dns_providers.rs (the `plugin`
# field of each provider). We ship only the supported plugins to stay lean.
#
# Usage: scripts/vendor-acme-dnsapi.sh [dest_dir]   (default packaging/acme.sh)
set -euo pipefail

# Pin to a released acme.sh tag for reproducibility + supply-chain safety. Bump
# deliberately and re-review the vendored scripts.
ACME_SH_REF="${ACME_SH_REF:-3.1.0}"
BASE="https://raw.githubusercontent.com/acmesh-official/acme.sh/${ACME_SH_REF}"

DEST="${1:-$(dirname "$0")/../packaging/acme.sh}"
PLUGINS=(cf aws dgon gandi_livedns desec namecheap gd vultr linode_v4 porkbun regru)

mkdir -p "$DEST/dnsapi"
echo "Vendoring acme.sh @ ${ACME_SH_REF} → $DEST"

curl -fsSL "$BASE/acme.sh" -o "$DEST/acme.sh"
chmod +x "$DEST/acme.sh"
echo "  acme.sh core"

for p in "${PLUGINS[@]}"; do
    curl -fsSL "$BASE/dnsapi/dns_${p}.sh" -o "$DEST/dnsapi/dns_${p}.sh"
    echo "  dns_${p}.sh"
done

# Provenance for the package (GPLv3 compliance / auditability).
printf 'acme.sh %s\nSource: https://github.com/acmesh-official/acme.sh (GPLv3)\nVendored: core + dnsapi/{%s}\n' \
    "$ACME_SH_REF" "$(IFS=,; echo "${PLUGINS[*]}")" > "$DEST/VENDOR.txt"

echo "Done: $(ls "$DEST/dnsapi" | wc -l | tr -d ' ') plugins + core."
