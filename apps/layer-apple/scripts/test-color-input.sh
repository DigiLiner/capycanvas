#!/bin/bash
set -euo pipefail
CAPY_APP="$(cd "$(dirname "$0")/.." && pwd)"
exec bash "$CAPY_APP/scripts/test-project-files.sh" "$CAPY_APP/tests/color-editor.swift"
