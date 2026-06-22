#!/usr/bin/env bash
set -euo pipefail

base_url="${BASE_URL:-http://127.0.0.1:3000}"
cookie_file="${COOKIE_FILE:-}"
samples="${SAMPLES:-5}"

if [[ -z "${cookie_file}" || ! -f "${cookie_file}" ]]; then
  echo "Set COOKIE_FILE to an authenticated curl cookie jar." >&2
  exit 1
fi

measure_route() {
  local route="$1"
  local output_file
  output_file="$(mktemp)"
  trap 'rm -f "${output_file}"' RETURN

  for _ in $(seq 1 "${samples}"); do
    curl \
      --silent \
      --show-error \
      --output /tmp/payplan-ui-baseline.html \
      --cookie "${cookie_file}" \
      --write-out '%{http_code}\t%{time_starttransfer}\t%{time_total}\t%{size_download}\n' \
      "${base_url}${route}" >>"${output_file}"
  done

  awk -F '\t' -v route="${route}" -v samples="${samples}" '
    {
      status = $1
      ttfb += $2
      total += $3
      bytes = $4
    }
    END {
      printf "%s\t%s\t%d\t%.6f\t%.6f\t%d\n",
        route, status, samples, ttfb / samples, total / samples, bytes
    }
  ' "${output_file}"
}

printf 'route\tstatus\tsamples\tmean_ttfb_seconds\tmean_total_seconds\thtml_bytes\n'
measure_route "/"
measure_route "/packages"
