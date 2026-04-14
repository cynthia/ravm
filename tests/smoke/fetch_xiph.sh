#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CACHE="${ROOT}/cache"
BASE_URL="${RUSTAVM_XIPH_BASE_URL:-https://media.xiph.org/video/aomctc/test_set/f2_still_MidRes}"
REPORT_URL="${RUSTAVM_XIPH_REPORT_URL:-${BASE_URL}/aomctc_stills_report.txt}"

mkdir -p "${CACHE}/ivf"

if ! command -v curl >/dev/null 2>&1; then
  echo "error: curl is required to fetch the Xiph smoke corpus" >&2
  exit 1
fi

FILES=(
  "Big_Easy_chair.ivf"
  "Washington_Monument.ivf"
  "Claudette.ivf"
  "KellermanBallfieldMefisto.ivf"
  "Fontaine_Place_Stanislas.ivf"
)

if [ ! -f "${CACHE}/aomctc_stills_report.txt" ]; then
  curl -fsSL "${REPORT_URL}" -o "${CACHE}/aomctc_stills_report.txt" || true
fi

for file in "${FILES[@]}"; do
  dst="${CACHE}/ivf/${file}"
  if [ -f "${dst}" ]; then
    echo "keep ${file}"
    continue
  fi
  echo "fetch ${file}"
  curl -fsSL "${BASE_URL}/ivf/${file}" -o "${dst}"
done
