# nxusKit Tier Comparison: Community vs Pro vs Enterprise

## Overview

nxusKit SDK uses a dual-edition model for the public SDK surface.

The public `nxusKit` repository contains nxusKit SDK Community Edition, which is free and open source. CE is not a trial, teaser, or time-limited evaluation; it is intended to remain useful on its own. We do not move released CE features behind the Pro paywall, and code released as CE remains available under its open-source license.

Community Edition is useful on its own without Pro. The public `nxusKit-examples` repository labels examples by edition, so developers can see which workflows run with CE alone and which require Pro.

nxusKit SDK Pro adds proprietary commercial capabilities for teams that need solver-backed workflows, ZEN decision tables, plugin loading, and trust-policy features. Pro is distributed under a paid, trial, or evaluation entitlement. **Enterprise** adds delegated trust and custom plugin configuration for large organizations.

## Feature Matrix

| Feature Domain | Community | Pro | Enterprise |
|----------------|:---------:|:---:|:----------:|
| LLM Cloud Providers (OpenAI, Claude, xAI Grok, Groq, Mistral, Fireworks, Together, OpenRouter, Perplexity) | Yes | Yes | Yes |
| LLM Local Providers (Ollama, LM Studio) | Yes | Yes | Yes |
| CLIPS Rule Engine (ClipsSession API) | Yes | Yes | Yes |
| Bayesian Network Inference | Yes | Yes | Yes |
| Auth Helper (API-key management, credential store) | Yes | Yes | Yes |
| Tool Calling / Function Calling | Yes | Yes | Yes |
| Streaming & Token Usage | Yes | Yes | Yes |
| Retry & Adaptive Rate Limiting | Yes | Yes | Yes |
| Vision / Image Support | Yes | Yes | Yes |
| OAuth Authentication Infrastructure | Yes | Yes | Yes |
| Cross-language Parity (Rust, Go, Python) | Yes | Yes | Yes |
| Static + Dynamic Linking | Yes | Yes | Yes |
| **ZEN Decision Tables** | — | Yes | Yes |
| **Constraint Solver (Z3-backed)** | — | Yes | Yes |
| **Plugin Loading & Trust Policy** | — | Yes | Yes |
| **MCP (Model Context Protocol)** | — | Yes | Yes |
| **CLIPS Advanced (programmatic rules, session persistence)** | — | Yes | Yes |
| **Custom Plugin Configuration Paths** | — | — | Yes |
| **Delegated Trust Roots** | — | — | Yes |
| **Priority Support** | — | — | Yes |

## Numerical Limits

| Limit | Community | Pro | Enterprise |
|-------|:---------:|:---:|:----------:|
| Max concurrent CLIPS sessions | 16 | 64 | 256 |
| Max cached rulebases | 8 | 32 | 128 |
| Max rules per session | 500 | 5,000 | 50,000 |
| Max facts per session | 10,000 | 100,000 | 1,000,000 |
| Max Bayesian network nodes | 50 | 500 | 5,000 |
| Max solver constraints | — | 10,000 | 100,000 |
| Machine activations (developer tokens) | — | 3 | Unlimited |

## SDK Wrapper Availability

All editions provide wrappers for all three languages:

| Language | Package | Install |
|----------|---------|---------|
| Rust | `nxuskit` | Path dependency from SDK bundle |
| Go | `nxuskit` | `go get github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go` |
| Python | `nxuskit-py` | SDK bundle `python/src` on `PYTHONPATH` |

## Example Tier Assignments

| Example | Tier | Reason |
|---------|------|--------|
| LLM basics, streaming, tool calling | Community | Uses cloud/local LLM providers |
| CLIPS basics, CLIPS-LLM hybrid | Community | CLIPS engine is Community-tier |
| Bayesian network inference | Community | BN inference is Community-tier |
| Gamer (game AI solver) | **Pro** | Uses Z3 constraint solver |
| Racer (optimization) | **Pro** | Uses Z3 constraint solver |
| Ruler (rule evaluation) | **Pro** | Uses ZEN decision tables |
| Solver (generic CSP) | **Pro** | Uses Z3 constraint solver |
| Riffer (music generation) | **Pro** | Uses solver + CLIPS pipeline |
| Sweeper (minesweeper AI) | **Pro** | Uses Z3 constraint solver |

## Licensing

| Aspect | Community | Pro | Enterprise |
|--------|-----------|-----|------------|
| License type | Open-source | Commercial subscription | Commercial subscription |
| Token required | No | Yes (developer or deployment) | Yes |
| Machine activations | N/A | Up to 3 per license | Unlimited |
| Deployment tokens | N/A | Unlimited instances, no per-seat fees | Unlimited |
| Version ceiling | N/A | Locked to major.minor at purchase time | Locked to major.minor |
| Trial | N/A | 30-day trial (registration required) | Contact sales |

## Getting Started with Pro

1. Create an account and register for a trial or purchase a license
2. Authenticate: `nxuskit-cli license login`
3. Activate on your machine: `nxuskit-cli license activate --key <purchase_id>`
4. For CI/CD: set `NXUSKIT_LICENSE_TOKEN` with your deployment token

See the [License Activation Guide](license-activation-guide.md) for the full workflow.
