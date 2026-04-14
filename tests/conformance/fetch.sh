#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
MANIFEST="${ROOT}/manifest.toml"
CACHE="${ROOT}/kf_only"
BASE_URL="${RUSTAVM_CONFORMANCE_BASE_URL:-https://storage.googleapis.com/aom-test-data}"

mkdir -p "${CACHE}"

if ! command -v curl >/dev/null 2>&1; then
  echo "error: curl is required to fetch conformance vectors" >&2
  exit 1
fi

mapfile -t FILES < <(grep -E '^file = "' "${MANIFEST}" | sed -E 's/^file = "(.*)"/\1/')

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "No vectors listed in ${MANIFEST}; nothing to fetch."
  exit 0
fi

for file in "${FILES[@]}"; do
  dst="${CACHE}/${file}"
  if [ -f "${dst}" ]; then
    echo "keep ${file}"
    continue
  fi
  echo "fetch ${file}"
  curl -fsSL "${BASE_URL}/${file}" -o "${dst}"
done
