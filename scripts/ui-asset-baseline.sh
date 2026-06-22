#!/usr/bin/env bash
set -euo pipefail

site_root="${1:-target/site}"
pkg_dir="${site_root}/pkg"

if [[ ! -d "${pkg_dir}" ]]; then
  echo "UI asset directory not found: ${pkg_dir}" >&2
  exit 1
fi

wasm="$(find "${pkg_dir}" -maxdepth 1 -type f -name '*.wasm' -print -quit)"
js="$(find "${pkg_dir}" -maxdepth 1 -type f -name '*.js' ! -name '__wasm_split*' -print -quit)"

if [[ -z "${wasm}" || -z "${js}" ]]; then
  echo "Expected release WASM and JavaScript assets in ${pkg_dir}" >&2
  exit 1
fi

file_size() {
  if stat -f%z "$1" >/dev/null 2>&1; then
    stat -f%z "$1"
  else
    stat -c%s "$1"
  fi
}

compressed_size() {
  local path="$1"
  if [[ -f "${path}" ]]; then
    file_size "${path}"
  else
    echo 0
  fi
}

asset_count="$(find "${pkg_dir}" -maxdepth 1 -type f | wc -l | tr -d ' ')"
asset_bytes="$(
  find "${pkg_dir}" -maxdepth 1 -type f -exec sh -c 'wc -c < "$1"' _ {} \; |
    awk '{ total += $1 } END { print total + 0 }'
)"

cat <<EOF
{
  "site_root": "${site_root}",
  "asset_count": ${asset_count},
  "asset_bytes": ${asset_bytes},
  "wasm": {
    "file": "$(basename "${wasm}")",
    "raw_bytes": $(file_size "${wasm}"),
    "gzip_bytes": $(compressed_size "${wasm}.gz"),
    "brotli_bytes": $(compressed_size "${wasm}.br")
  },
  "javascript": {
    "file": "$(basename "${js}")",
    "raw_bytes": $(file_size "${js}"),
    "gzip_bytes": $(compressed_size "${js}.gz"),
    "brotli_bytes": $(compressed_size "${js}.br")
  }
}
EOF
