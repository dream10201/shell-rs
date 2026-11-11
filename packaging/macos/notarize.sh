#!/usr/bin/env bash
# Have Apple notarize a zip or disk image, and fail unless it is accepted.
#
#   packaging/macos/notarize.sh <file>
#
# $RUNNER_TEMP/AuthKey.p8 is the App Store Connect API key; API_KEY_ID and
# API_ISSUER name it.
set -euo pipefail

key="$RUNNER_TEMP/AuthKey.p8"
xcrun notarytool submit "$1" --key "$key" --key-id "$API_KEY_ID" --issuer "$API_ISSUER" \
    --wait --output-format json > "$RUNNER_TEMP/notary.json"
cat "$RUNNER_TEMP/notary.json"
# --wait returning is not success: the status has to say so.
if [ "$(jq -r .status "$RUNNER_TEMP/notary.json")" != Accepted ]; then
    xcrun notarytool log "$(jq -r .id "$RUNNER_TEMP/notary.json")" \
        --key "$key" --key-id "$API_KEY_ID" --issuer "$API_ISSUER"
    exit 1
fi
