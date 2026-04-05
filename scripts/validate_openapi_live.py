#!/usr/bin/env python3
"""Run aios-api, fetch /openapi.json, and validate it against OpenAPI schema."""

from __future__ import annotations

import argparse
import json
import signal
import subprocess
import sys
import time
import urllib.request
from pathlib import Path
from typing import Any


def _fetch_json(url: str) -> dict[str, Any]:
    with urllib.request.urlopen(url) as response:  # noqa: S310 - URL comes from CLI input.
        return json.loads(response.read().decode("utf-8"))


def _wait_for_health(url: str, timeout_secs: float) -> bool:
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url) as response:  # noqa: S310
                if response.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(0.25)
    return False


def _terminate(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(description="Live OpenAPI validation for aiOS API.")
    parser.add_argument("--listen", default="127.0.0.1:8788")
    parser.add_argument("--runtime-root", default=".aios-openapi-validate")
    parser.add_argument("--server-log", default="/tmp/aios-openapi-server.log")
    parser.add_argument("--openapi-file", default="target/openapi.json")
    parser.add_argument("--health-timeout-secs", type=float, default=20.0)
    args = parser.parse_args()

    try:
        from openapi_spec_validator import validate_spec
    except ModuleNotFoundError:
        print(
            "error: missing dependency 'openapi-spec-validator'. "
            "Install with: python3 -m pip install openapi-spec-validator",
            file=sys.stderr,
        )
        return 2

    server_log = Path(args.server_log)
    server_log.parent.mkdir(parents=True, exist_ok=True)

    command = [
        "cargo",
        "run",
        "--locked",
        "-p",
        "aios-api",
        "--",
        "--root",
        args.runtime_root,
        "--listen",
        args.listen,
    ]

    with server_log.open("w", encoding="utf-8") as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, text=True)

    try:
        if not _wait_for_health(f"http://{args.listen}/healthz", args.health_timeout_secs):
            print(
                f"error: aios-api did not become healthy on {args.listen}",
                file=sys.stderr,
            )
            if server_log.exists():
                print("--- server log ---", file=sys.stderr)
                print(server_log.read_text(encoding="utf-8"), file=sys.stderr)
            return 1

        document = _fetch_json(f"http://{args.listen}/openapi.json")
        openapi_version = document.get("openapi")
        if not isinstance(openapi_version, str) or not openapi_version.startswith("3.1."):
            print(
                f"error: expected OpenAPI 3.1.x, got {openapi_version!r}",
                file=sys.stderr,
            )
            return 1

        validate_spec(document)

        output_path = Path(args.openapi_file)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(document, indent=2), encoding="utf-8")
        print(f"OpenAPI validation succeeded (version {openapi_version}).")
        return 0
    except Exception as error:
        print(f"error: OpenAPI schema validation failed: {error}", file=sys.stderr)
        return 1
    finally:
        _terminate(process)


if __name__ == "__main__":
    raise SystemExit(main())
