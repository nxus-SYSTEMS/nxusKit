"""nxuskit SDK Example — CLIPS Basics (Python)

Demonstrates: CLIPS provider creation, rule loading, fact assertion,
inference engine execution, and result querying via the ClipsSession API.

Run:
    export CLIPS_RULES_DIR="/path/to/rules"  # directory with .clp files
    python clips_basics.py

Prerequisites:
    export NXUSKIT_SDK_DIR="/path/to/nxuskit-sdk-1.0.4-pro-<platform>"
    export PYTHONPATH="$NXUSKIT_SDK_DIR/python/src:${PYTHONPATH:-}"

Tier: Pro (requires nxusKit Pro license)
"""

import json
import os
import sys

from nxuskit._ffi_provider import create_ffi_provider
from nxuskit._ffi_errors import ConfigError, ProviderError


# ── Animal Classification ─────────────────────────────────────────

def classify_animal(provider, animal_data: dict) -> dict:
    """Classify an animal using CLIPS expert system rules.

    Sends animal characteristics as facts, runs the inference engine,
    and returns the classification result.
    """
    request = {
        "model": "animal-classification",
        "messages": [
            {
                "role": "user",
                "content": json.dumps({
                    "facts": [
                        {
                            "template": "animal",
                            "values": animal_data,
                        }
                    ],
                    "config": {
                        "include_trace": True,
                    },
                }),
            }
        ],
    }

    response = provider.chat(request)
    return json.loads(response.content)


def demo_animal_classification(provider):
    """Run animal classification examples."""
    animals = [
        {
            "name": "dog",
            "has_backbone": True,
            "blood_type": "warm",
            "skin_covering": "fur",
            "legs": 4,
            "lays_eggs": False,
            "can_fly": False,
        },
        {
            "name": "eagle",
            "has_backbone": True,
            "blood_type": "warm",
            "skin_covering": "feathers",
            "legs": 2,
            "lays_eggs": True,
            "can_fly": True,
        },
        {
            "name": "salmon",
            "has_backbone": True,
            "blood_type": "cold",
            "skin_covering": "scales",
            "legs": 0,
            "lays_eggs": True,
            "can_fly": False,
        },
    ]

    print("=== Animal Classification ===\n")

    for animal in animals:
        result = classify_animal(provider, animal)

        conclusions = result.get("conclusions", [])
        stats = result.get("stats", {})

        print(f"Animal: {animal['name']}")
        if conclusions:
            for c in conclusions:
                if c.get("template") == "classification":
                    values = c.get("values", {})
                    print(f"  Category:   {values.get('category', 'unknown')}")
                    print(f"  Sub-class:  {values.get('sub_class', 'N/A')}")
                    print(f"  Confidence: {values.get('confidence', 0):.0%}")
        print(f"  Rules fired: {stats.get('total_rules_fired', 0)}")
        print()


# ── Main ──────────────────────────────────────────────────────────

def main():
    rules_dir = os.environ.get("CLIPS_RULES_DIR")
    if not rules_dir:
        print(
            "Error: set CLIPS_RULES_DIR to a directory containing .clp rule files",
            file=sys.stderr,
        )
        print("Example: export CLIPS_RULES_DIR=./rules", file=sys.stderr)
        sys.exit(1)

    print("nxusKit CLIPS Basics — Python Example")
    print(f"Rules directory: {rules_dir}\n")

    with create_ffi_provider({
        "provider_type": "clips",
        "rules_dir": rules_dir,
    }) as provider:
        demo_animal_classification(provider)

    print("Done.")


if __name__ == "__main__":
    try:
        main()
    except ConfigError as e:
        print(f"Configuration error: {e.message}", file=sys.stderr)
        sys.exit(1)
    except ProviderError as e:
        print(f"Provider error: {e.message}", file=sys.stderr)
        sys.exit(1)
