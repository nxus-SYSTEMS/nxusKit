//! Simple RAG (Retrieval-Augmented Generation) Example
//!
//! Demonstrates the RAG pattern using:
//! 1. **Retrieval**: TF-IDF keyword search (pure Rust, no external deps)
//! 2. **Generation**: nxusKit chat API call with retrieved context
//!
//! The retrieval and generation stages are clearly separated.

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════
// Stage 1: Retrieval — TF-IDF search (no nxusKit dependency)
// ═══════════════════════════════════════════════════════════════════

/// A document in the corpus.
struct Document {
    title: &'static str,
    content: &'static str,
}

/// Embedded corpus of technology FAQs.
fn corpus() -> Vec<Document> {
    vec![
        Document {
            title: "What is Rust?",
            content: "Rust is a systems programming language focused on safety, speed, and \
                      concurrency. It achieves memory safety without garbage collection through \
                      its ownership system. Rust is used for web servers, CLI tools, game engines, \
                      operating systems, and embedded devices.",
        },
        Document {
            title: "What is a rule engine?",
            content: "A rule engine evaluates business rules against data (facts) to derive \
                      conclusions. CLIPS is a classic forward-chaining rule engine. Rules are \
                      if-then statements that fire when their conditions match the current facts. \
                      Rule engines are used in expert systems, compliance checking, and automation.",
        },
        Document {
            title: "What is RAG?",
            content: "RAG (Retrieval-Augmented Generation) is a technique that combines information \
                      retrieval with language model generation. First, relevant documents are \
                      retrieved from a knowledge base using search. Then, the retrieved context is \
                      passed to an LLM along with the user's question to generate a grounded answer.",
        },
        Document {
            title: "What is constraint solving?",
            content: "Constraint solving finds values for variables that satisfy a set of \
                      constraints. Z3 is an SMT solver from Microsoft Research that can solve \
                      integer, real, boolean, and bitvector constraints. It is used for program \
                      verification, test generation, and optimization problems.",
        },
        Document {
            title: "What is a Bayesian network?",
            content: "A Bayesian network is a probabilistic graphical model representing variables \
                      and their conditional dependencies via a directed acyclic graph. Inference \
                      algorithms compute posterior probabilities given evidence. Applications include \
                      medical diagnosis, risk assessment, and anomaly detection.",
        },
        Document {
            title: "What is nxusKit?",
            content: "nxusKit is a multi-engine AI SDK that provides a unified API across LLM \
                      providers, rule engines (CLIPS), constraint solvers (Z3), Bayesian networks, \
                      and decision tables (ZEN). It supports Rust, Go, and Python with a shared \
                      C ABI core. nxusKit handles provider abstraction, entitlement gating, and \
                      session management.",
        },
        Document {
            title: "What is TF-IDF?",
            content: "TF-IDF (Term Frequency-Inverse Document Frequency) is a text relevance \
                      scoring method. TF measures how often a term appears in a document. IDF \
                      measures how rare a term is across all documents. The product TF * IDF \
                      gives high scores to terms that are frequent in a document but rare overall, \
                      making it useful for keyword-based search and information retrieval.",
        },
    ]
}

/// Tokenize text into lowercase words.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(String::from)
        .collect()
}

/// Compute term frequencies for a token list.
fn term_frequency(tokens: &[String]) -> HashMap<String, f64> {
    let mut counts: HashMap<String, f64> = HashMap::new();
    let len = tokens.len() as f64;
    for token in tokens {
        *counts.entry(token.clone()).or_default() += 1.0;
    }
    for val in counts.values_mut() {
        *val /= len;
    }
    counts
}

/// Compute IDF for each term across a corpus of token sets.
fn inverse_document_frequency(docs: &[Vec<String>]) -> HashMap<String, f64> {
    let n = docs.len() as f64;
    let mut df: HashMap<String, f64> = HashMap::new();
    for doc_tokens in docs {
        let unique: std::collections::HashSet<&str> =
            doc_tokens.iter().map(|s| s.as_str()).collect();
        for term in unique {
            *df.entry(term.to_string()).or_default() += 1.0;
        }
    }
    df.into_iter()
        .map(|(term, count)| (term, (n / count).ln()))
        .collect()
}

/// Score a query against the corpus using TF-IDF. Returns (index, score) pairs
/// sorted by descending relevance.
fn tfidf_search(query: &str, documents: &[Document], top_k: usize) -> Vec<(usize, f64)> {
    let query_tokens = tokenize(query);
    let doc_token_sets: Vec<Vec<String>> = documents
        .iter()
        .map(|d| tokenize(&format!("{} {}", d.title, d.content)))
        .collect();
    let idf = inverse_document_frequency(&doc_token_sets);

    let mut scores: Vec<(usize, f64)> = doc_token_sets
        .iter()
        .enumerate()
        .map(|(idx, doc_tokens)| {
            let tf = term_frequency(doc_tokens);
            let score: f64 = query_tokens
                .iter()
                .map(|qt| {
                    let tf_val = tf.get(qt).copied().unwrap_or(0.0);
                    let idf_val = idf.get(qt).copied().unwrap_or(0.0);
                    tf_val * idf_val
                })
                .sum();
            (idx, score)
        })
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(top_k);
    scores
}

// ═══════════════════════════════════════════════════════════════════
// Stage 2: Generation — nxusKit chat API (requires LLM provider)
// ═══════════════════════════════════════════════════════════════════

fn main() {
    let docs = corpus();
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "How does Rust achieve memory safety?".to_string());

    println!("=== Simple RAG Example ===\n");
    println!("Query: {query}\n");

    // --- Retrieval stage (no LLM needed) ---
    println!("--- Retrieval (TF-IDF) ---\n");
    let results = tfidf_search(&query, &docs, 3);

    if results.is_empty() || results[0].1 == 0.0 {
        println!("No relevant documents found.");
        return;
    }

    let mut context_snippets = Vec::new();
    for (idx, score) in &results {
        if *score > 0.0 {
            println!("  [{:.4}] {}", score, docs[*idx].title);
            context_snippets.push(format!("[{}] {}", docs[*idx].title, docs[*idx].content));
        }
    }

    let context = context_snippets.join("\n\n");
    println!("\n--- Retrieved context ({} snippets) ---\n", context_snippets.len());

    // --- Generation stage (requires LLM provider) ---
    println!("--- Generation (nxusKit chat) ---\n");

    let system_msg = format!(
        "You are a helpful assistant. Answer the user's question using ONLY the following context. \
         If the context doesn't contain relevant information, say so.\n\nContext:\n{context}"
    );

    // Build the chat request using nxusKit types
    let messages = vec![
        nxuskit::Message::system(&system_msg),
        nxuskit::Message::user(&query),
    ];

    println!("Chat request built with {} messages.", messages.len());
    println!("System message includes {} retrieved snippets.", context_snippets.len());
    println!("\nTo generate a response, configure an LLM provider:");
    println!("  export ANTHROPIC_API_KEY=<your-key>");
    println!("  # Then modify this example to call NxuskitProvider::chat()");
    println!("\nRetrieval stage completed successfully (no LLM call needed).");
}

// ═══════════════════════════════════════════════════════════════════
// Tests — retrieval-only (no LLM provider required)
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tfidf_retrieval_returns_relevant_results() {
        let docs = corpus();
        let results = tfidf_search("Rust memory safety", &docs, 3);

        assert!(!results.is_empty(), "should return at least one result");
        let top_idx = results[0].0;
        let top_content = format!("{} {}", docs[top_idx].title, docs[top_idx].content);
        assert!(
            top_content.to_lowercase().contains("rust"),
            "top result for 'Rust memory safety' should mention Rust, got: {}",
            docs[top_idx].title
        );
    }

    #[test]
    fn tfidf_retrieval_rule_engine_query() {
        let docs = corpus();
        let results = tfidf_search("rule engine CLIPS", &docs, 3);

        assert!(!results.is_empty());
        let top_idx = results[0].0;
        let top_content = format!("{} {}", docs[top_idx].title, docs[top_idx].content);
        assert!(
            top_content.to_lowercase().contains("rule"),
            "top result for 'rule engine CLIPS' should mention rules"
        );
    }

    #[test]
    fn tfidf_scores_are_non_negative() {
        let docs = corpus();
        let results = tfidf_search("anything", &docs, 10);
        for (_, score) in &results {
            assert!(*score >= 0.0, "scores should be non-negative");
        }
    }
}
