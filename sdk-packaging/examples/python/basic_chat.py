"""nxuskit SDK Example — Basic Chat (Python)

Demonstrates: provider creation, synchronous chat, response reading, cleanup.

Run:
    export OPENAI_API_KEY="sk-..."
    python basic_chat.py

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

    # Create an OpenAI provider
    with create_ffi_provider({
        "provider_type": "openai",
        "api_key": api_key,
    }) as provider:
        # Send a chat request
        response = provider.chat({
            "model": "gpt-4o-mini",
            "messages": [
                {"role": "user", "content": "What is the capital of France? Reply in one sentence."}
            ],
            "max_tokens": 100,
        })

        print(f"Response: {response.content}")
        print(f"Model: {response.model}")

        if response.usage:
            print(
                f"Tokens: {response.usage.prompt_tokens} prompt "
                f"+ {response.usage.completion_tokens} completion "
                f"= {response.usage.total_tokens} total"
            )


if __name__ == "__main__":
    try:
        main()
    except ConfigError as e:
        print(f"Configuration error: {e.message}", file=sys.stderr)
        sys.exit(1)
    except ProviderError as e:
        print(f"Provider error: {e.message}", file=sys.stderr)
        sys.exit(1)
