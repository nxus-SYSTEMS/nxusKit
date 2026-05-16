"""nxuskit SDK Example — CLIPS-LLM Hybrid (Python)

Demonstrates: combining CLIPS rule-based reasoning with LLM intelligence
in a support ticket routing pipeline.

Three-step hybrid pattern:
  1. LLM classifies the ticket (category, priority, sentiment)
  2. CLIPS routes based on classification (team, SLA, escalation)
  3. LLM generates a response using routing context

Run:
    export OPENAI_API_KEY="sk-..."  # or any LLM provider key
    export CLIPS_RULES_DIR="/path/to/rules"
    python clips_llm_hybrid.py

Prerequisites:
    pip install nxuskit-py

Tier: Pro (requires nxusKit Pro license)
"""

import json
import os
import sys
from dataclasses import dataclass
from typing import Optional

from nxuskit._ffi_provider import create_ffi_provider
from nxuskit._ffi_errors import ConfigError, ProviderError


# ── Data Types ────────────────────────────────────────────────────

@dataclass
class TicketClassification:
    """LLM-derived ticket classification."""
    category: str       # security, infrastructure, application, general
    priority: str       # critical, high, medium, low
    sentiment: str      # frustrated, neutral, satisfied
    entities: list       # extracted key entities

@dataclass
class RoutingDecision:
    """CLIPS-derived routing decision."""
    team: str           # security-ops, sre, dev, support
    sla_hours: int      # response SLA in hours
    escalation_level: str  # immediate, standard, normal

@dataclass
class TicketAnalysis:
    """Combined analysis result."""
    classification: TicketClassification
    routing: RoutingDecision
    summary: Optional[str] = None


# ── Step 1: LLM Classification ───────────────────────────────────

CLASSIFICATION_PROMPT = """Classify this support ticket. Return ONLY valid JSON with these fields:
{
  "category": "security|infrastructure|application|general",
  "priority": "critical|high|medium|low",
  "sentiment": "frustrated|neutral|satisfied",
  "entities": ["list", "of", "key", "entities"]
}

Ticket: {ticket_text}"""


def classify_ticket(llm_provider, ticket_text: str) -> TicketClassification:
    """Use LLM to classify ticket category, priority, and sentiment."""
    response = llm_provider.chat({
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "user", "content": CLASSIFICATION_PROMPT.format(ticket_text=ticket_text)},
        ],
        "max_tokens": 200,
    })

    try:
        data = json.loads(response.content)
    except json.JSONDecodeError:
        # Fallback for non-JSON responses
        data = {
            "category": "general",
            "priority": "medium",
            "sentiment": "neutral",
            "entities": [],
        }

    return TicketClassification(
        category=data.get("category", "general"),
        priority=data.get("priority", "medium"),
        sentiment=data.get("sentiment", "neutral"),
        entities=data.get("entities", []),
    )


# ── Step 2: CLIPS Routing ────────────────────────────────────────

def apply_routing_rules(clips_provider, classification: TicketClassification) -> RoutingDecision:
    """Use CLIPS expert system to determine routing based on classification."""
    request = {
        "model": "ticket-routing",
        "messages": [
            {
                "role": "user",
                "content": json.dumps({
                    "facts": [
                        {
                            "template": "ticket-classification",
                            "values": {
                                "category": classification.category,
                                "priority": classification.priority,
                                "sentiment": classification.sentiment,
                            },
                        }
                    ],
                    "config": {"include_trace": True},
                }),
            }
        ],
    }

    response = clips_provider.chat(request)
    result = json.loads(response.content)

    # Extract routing decision from CLIPS conclusions
    for conclusion in result.get("conclusions", []):
        if conclusion.get("template") == "routing-decision":
            values = conclusion["values"]
            return RoutingDecision(
                team=values.get("team", "support"),
                sla_hours=int(values.get("sla-hours", 24)),
                escalation_level=values.get("escalation-level", "normal"),
            )

    # Default routing if no rule matched
    return RoutingDecision(team="support", sla_hours=24, escalation_level="normal")


# ── Step 3: Hybrid Pipeline ──────────────────────────────────────

def analyze_ticket(llm_provider, clips_provider, ticket_text: str) -> TicketAnalysis:
    """Full hybrid pipeline: LLM classify → CLIPS route → combine."""
    # Step 1: LLM classification
    classification = classify_ticket(llm_provider, ticket_text)

    # Step 2: CLIPS routing
    routing = apply_routing_rules(clips_provider, classification)

    return TicketAnalysis(
        classification=classification,
        routing=routing,
    )


# ── Test Tickets ──────────────────────────────────────────────────

TEST_TICKETS = [
    "URGENT: Our API keys may have been exposed in a public GitHub repo. "
    "Multiple services are affected. Need immediate security review.",

    "The production database is running at 98% capacity and response times "
    "have tripled in the last hour. We need to scale up immediately.",

    "Users are reporting that the export to PDF feature returns a blank page "
    "when the report contains more than 50 rows. Started after last deploy.",

    "Hi, I was wondering if you could help me understand how to set up "
    "two-factor authentication on my account? Thanks!",
]


# ── Main ──────────────────────────────────────────────────────────

def main():
    api_key = os.environ.get("OPENAI_API_KEY")
    rules_dir = os.environ.get("CLIPS_RULES_DIR")

    if not api_key:
        print("Error: set OPENAI_API_KEY environment variable", file=sys.stderr)
        sys.exit(1)
    if not rules_dir:
        print("Error: set CLIPS_RULES_DIR environment variable", file=sys.stderr)
        sys.exit(1)

    print("nxusKit CLIPS-LLM Hybrid — Python Example")
    print("=" * 50)
    print()

    with create_ffi_provider({
        "provider_type": "openai",
        "api_key": api_key,
    }) as llm_provider, create_ffi_provider({
        "provider_type": "clips",
        "rules_dir": rules_dir,
    }) as clips_provider:

        for i, ticket in enumerate(TEST_TICKETS, 1):
            print(f"--- Ticket {i} ---")
            print(f"Text: {ticket[:80]}...")
            print()

            analysis = analyze_ticket(llm_provider, clips_provider, ticket)

            c = analysis.classification
            r = analysis.routing

            print(f"  Classification:")
            print(f"    Category:  {c.category}")
            print(f"    Priority:  {c.priority}")
            print(f"    Sentiment: {c.sentiment}")
            print(f"    Entities:  {', '.join(c.entities) if c.entities else 'none'}")
            print(f"  Routing:")
            print(f"    Team:       {r.team}")
            print(f"    SLA:        {r.sla_hours}h")
            print(f"    Escalation: {r.escalation_level}")
            print()

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
