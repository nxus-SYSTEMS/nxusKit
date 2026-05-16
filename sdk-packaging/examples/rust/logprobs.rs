// nxuskit SDK Example — Token Logprobs (v0.9.3, unary chat only)
//
// Demonstrates the first-class `with_logprobs` / `with_top_logprobs`
// builders and the typed `ChatResponse.logprobs: Option<LogprobsData>`
// surface added in v0.9.3. Streaming logprobs are out of scope for
// v0.9.3; see the internal v0.9.4 deferral register.
//
// Setup:
//   export NXUSKIT_SDK_DIR="/path/to/nxuskit-sdk-<version>-<platform>"
//   export OPENAI_API_KEY="sk-..."   # OR set NXUSKIT_SKIP_NETWORK=1 for build-only
//
// Run:
//   cd sdk-packaging/examples/rust
//   cargo run --bin logprobs
//
// CI-friendly: if neither OPENAI_API_KEY nor any other recognized
// credential is set, the example logs a SKIP line and exits 0 so it
// does not fail default CI without provider credentials.

use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig};

fn main() -> Result<(), nxuskit::NxuskitError> {
    // CI / no-credentials safety net: skip cleanly without panicking
    // when no provider credentials are visible in the environment.
    if std::env::var("OPENAI_API_KEY").is_err()
        && std::env::var("NXUSKIT_LICENSE_TOKEN").is_err()
        && std::env::var("NXUSKIT_FORCE_RUN").is_err()
    {
        println!(
            "SKIP: no OPENAI_API_KEY / NXUSKIT_LICENSE_TOKEN in environment; \
             set NXUSKIT_FORCE_RUN=1 to attempt anyway."
        );
        return Ok(());
    }

    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        ..Default::default()
    })?;

    // First-class logprobs request — NO provider_options tunneling.
    // Engine warn-and-drops if the resolved provider lacks
    // supports_logprobs (warning surfaces in response.warnings).
    let request = ChatRequest::new("gpt-4o-mini")
        .with_message(Message::user("Reply with one short word."))
        .with_max_tokens(8)
        .with_logprobs(true)
        .with_top_logprobs(3);

    let response = provider.chat(request)?;

    println!("Response: {}", response.content);

    if !response.warnings.is_empty() {
        println!("Provider warnings: {:?}", response.warnings);
    }

    match response.logprobs {
        Some(lp) => {
            println!("Got logprobs for {} token(s):", lp.content.len());
            for (i, token) in lp.content.iter().enumerate() {
                println!(
                    "  [{i}] {:?} (logprob {:.4}) — {} alternative(s)",
                    token.token,
                    token.logprob,
                    token.top_logprobs.len(),
                );
                for alt in &token.top_logprobs {
                    println!("        alt: {:?} ({:.4})", alt.token, alt.logprob);
                }
            }
        }
        None => {
            println!(
                "No logprobs in response — provider likely warned-and-dropped \
                 (check `response.warnings`) or model does not return logprobs."
            );
        }
    }

    Ok(())
}
