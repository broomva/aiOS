#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
listen_addr="${AIOS_OPENAPI_LISTEN:-127.0.0.1:8788}"
runtime_root="${AIOS_OPENAPI_ROOT:-${repo_root}/.aios-openapi-validate}"
openapi_file="${repo_root}/target/openapi.json"
server_log="${AIOS_OPENAPI_SERVER_LOG:-/tmp/aios-openapi-server.log}"
python_bin="${AIOS_PYTHON_BIN:-python3}"

"${python_bin}" "${repo_root}/scripts/validate_openapi_live.py" \
  --listen "${listen_addr}" \
  --runtime-root "${runtime_root}" \
  --openapi-file "${openapi_file}" \
  --server-log "${server_log}"
