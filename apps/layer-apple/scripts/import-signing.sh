#!/bin/bash
set -euo pipefail
base64 --decode <<< "${API_KEY:?}" > "$RUNNER_TEMP/api-key.p8"
{
  echo "CAPY_APPLE_TEAM=${APPLE_TEAM_ID:?}"
  echo "CAPY_APPLE_KEY=$RUNNER_TEMP/api-key.p8"
  echo "CAPY_APPLE_KEY_ID=${APPLE_API_KEY_ID:?}"
  echo "CAPY_APPLE_ISSUER=${APPLE_API_ISSUER_ID:?}"
} >> "$GITHUB_ENV"
keychain="$RUNNER_TEMP/signing.keychain-db"
base64 --decode <<< "${SIGNING_CERTIFICATE:?}" > "$RUNNER_TEMP/signing.p12"
security create-keychain -p '' "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p '' "$keychain"
security import "$RUNNER_TEMP/signing.p12" -P "${SIGNING_PASSWORD:?}" -A -t cert -f pkcs12 -k "$keychain"
security set-key-partition-list -S apple-tool:,apple: -k '' "$keychain"
security list-keychains -d user -s "$keychain"
if ! security find-identity -v -p codesigning "$keychain" | grep -Fq "\"${SIGNING_IDENTITY:?}:"; then
  echo "The signing certificate must include a valid $SIGNING_IDENTITY private key." >&2
  exit 1
fi
