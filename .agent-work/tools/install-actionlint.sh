#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$root/bin"
GOBIN="$root/bin" go install github.com/rhysd/actionlint/cmd/actionlint@latest
"$root/bin/actionlint" -version
