# Simple RAG Example

A self-contained Retrieval-Augmented Generation (RAG) example using:

1. **TF-IDF retrieval** — pure Rust keyword search (no external dependencies)
2. **nxusKit chat** — LLM generation with retrieved context

## How It Works

The RAG pattern has two stages:

- **Retrieval**: Given a query, find the most relevant documents from a
  corpus using TF-IDF scoring. This stage uses only standard Rust — no
  LLM provider is needed.

- **Generation**: Pass the retrieved context snippets along with the user's
  question to an LLM via nxusKit's chat API. The LLM generates an answer
  grounded in the retrieved context.

## Running

```bash
# Default query
cargo run

# Custom query
cargo run -- "What is a Bayesian network?"
```

## Testing (retrieval only, no LLM needed)

```bash
cargo test
```

The tests verify that TF-IDF retrieval returns semantically relevant results
without requiring an LLM provider.

## How nxusKit Fits In

nxusKit provides the generation stage via its unified LLM provider API.
The retrieval stage is intentionally provider-agnostic — you could swap
TF-IDF for vector embeddings, a search API, or any other retrieval method.
nxusKit handles the LLM call, provider abstraction, and token management.
