#!/usr/bin/env python3
"""Validate an OpenAPI JSON document with openapi-spec-validator."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path
from typing import Any


def _load_from_file(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _load_from_url(url: str) -> dict[str, Any]:
    with urllib.request.urlopen(url) as response:  # noqa: S310 - URL is user-provided CLI input.
        return json.loads(response.read().decode("utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate OpenAPI JSON.")
    parser.add_argument("--file", type=Path, help="Path to OpenAPI JSON file")
    parser.add_argument("--url", help="URL for OpenAPI JSON document")
    args = parser.parse_args()

    if bool(args.file) == bool(args.url):
        print("error: provide exactly one of --file or --url", file=sys.stderr)
        return 2

    try:
        from openapi_spec_validator import validate_spec
    except ModuleNotFoundError:
        print(
            "error: missing dependency 'openapi-spec-validator'. "
            "Install with: python3 -m pip install openapi-spec-validator "
            "(or set AIOS_PYTHON_BIN to a virtualenv Python that has it installed).",
            file=sys.stderr,
        )
        return 2

    try:
        document = _load_from_file(args.file) if args.file else _load_from_url(args.url)
    except Exception as error:
        print(f"error: failed to load OpenAPI document: {error}", file=sys.stderr)
        return 1

    openapi_version = document.get("openapi")
    if not isinstance(openapi_version, str) or not openapi_version.startswith("3.1."):
        print(
            f"error: expected OpenAPI 3.1.x, got {openapi_version!r}",
            file=sys.stderr,
        )
        return 1

    try:
        validate_spec(document)
    except Exception as error:
        print(f"error: OpenAPI schema validation failed: {error}", file=sys.stderr)
        return 1

    print(f"OpenAPI validation succeeded (version {openapi_version}).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
