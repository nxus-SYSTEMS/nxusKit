"""nxuskit SDK Example — Streaming Chat (Python)

Demonstrates: streaming chat with iterator pattern, chunk processing.

Run:
    export OPENAI_API_KEY="sk-..."
    python streaming.py

Prerequisites:
    export NXUSKIT_SDK_DIR="/path/to/nxuskit-sdk-1.0.4-oss-<platform>"
    export PYTHONPATH="$NXUSKIT_SDK_DIR/python/src:${PYTHONPATH:-}"
"""

import os
import sys

from nxuskit._ffi_provider import create_ffi_provider
from nxuskit._ffi_errors import ConfigError, ProviderError


def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        print("Error: set OPENAI_API_KEY environment variable", file=sys.stderr)
        sys.exit(1)

    with create_ffi_provider({
        "provider_type": "openai",
        "api_key": api_key,
    }) as provider:
        print("Streaming: ", end="", flush=True)

        # Stream yields chunks as they arrive
        for chunk in provider.stream({
            "model": "gpt-4o-mini",
            "messages": [
                {"role": "user", "content": "Count from 1 to 5, with a brief description for each number."}
            ],
            "max_tokens": 200,
            "stream": True,
        }):
            print(chunk.content, end="", flush=True)

        print("\n\nDone.")


if __name__ == "__main__":
    try:
        main()
    except ConfigError as e:
        print(f"\nConfiguration error: {e.message}", file=sys.stderr)
        sys.exit(1)
    except ProviderError as e:
        print(f"\nProvider error: {e.message}", file=sys.stderr)
        sys.exit(1)
