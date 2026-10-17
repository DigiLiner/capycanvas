#!/bin/bash
set -euo pipefail
CAPY_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
case "${PLATFORM_NAME:-iphoneos}" in
  iphoneos) CAPY_TARGET=aarch64-apple-ios ;;
  iphonesimulator) CAPY_TARGET=aarch64-apple-ios-sim ;;
  macosx) CAPY_TARGET=aarch64-apple-darwin ;;
  *) echo "Unsupported Apple platform: ${PLATFORM_NAME}" >&2; exit 1 ;;
esac
cd "$CAPY_ROOT"
CAPY_PROFILE=dev
if [[ "${CONFIGURATION:-Debug}" == Release ]]; then CAPY_PROFILE=release; fi
if [[ "$CAPY_TARGET" != aarch64-apple-darwin ]]; then
  : "${IPHONEOS_DEPLOYMENT_TARGET:?Set the iOS deployment target before building}"
  export CARGO_TARGET_DIR="${CAPY_RUST_BUILD_ROOT:-$CAPY_ROOT/target/ios-$IPHONEOS_DEPLOYMENT_TARGET}"
  # Cross builds resolve host and iOS target SDKs independently.
  unset SDKROOT
elif [[ -n "${CAPY_RUST_BUILD_ROOT:-}" ]]; then
  export CARGO_TARGET_DIR="$CAPY_RUST_BUILD_ROOT"
fi
cargo build --locked -p layer-apple-link --target "$CAPY_TARGET" --profile "$CAPY_PROFILE"
