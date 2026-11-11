#!/usr/bin/env python3
"""Decide whether the all-platform SDK workflow may publish a release."""

from __future__ import annotations

import argparse
import os
import re
from pathlib import Path


CELL_ARGUMENTS = (
    "linux-oss",
    "macos-oss",
    "macos-x86-64-oss",
    "windows-oss",
    "linux-pro",
    "macos-pro",
    "macos-x86-64-pro",
    "windows-pro",
)
RELEASE_TAG = re.compile(r"sdk-v[0-9]+\.[0-9]+\.[0-9]+")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event", required=True)
    parser.add_argument("--create-release", required=True)
    parser.add_argument("--requested-ref", required=True)
    parser.add_argument("--validation", required=True)
    for cell in CELL_ARGUMENTS:
        parser.add_argument(f"--{cell}", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    cell_states = [getattr(args, cell.replace("-", "_")) for cell in CELL_ARGUMENTS]
    eligible = (
        args.event == "workflow_dispatch"
        and args.create_release == "true"
        and args.validation == "success"
        and RELEASE_TAG.fullmatch(args.requested_ref) is not None
        and all(state == "success" for state in cell_states)
    )

    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        raise SystemExit("GITHUB_OUTPUT is required")
    with Path(output_path).open("a", encoding="utf-8") as output:
        output.write(f"eligible={'true' if eligible else 'false'}\n")

    print("SDK release publication eligible" if eligible else "SDK release publication blocked")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
