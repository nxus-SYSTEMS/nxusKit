// nxuskit SDK Example — Basic Chat (Rust)
//
// Uses the bundled nxuskit crate as a path dependency.
//
// Setup:
//   export NXUSKIT_SDK_DIR="/path/to/nxuskit-sdk-<version>-<platform>"
//   export OPENAI_API_KEY="sk-..."
//
// Run:
//   cd examples/rust
//   cargo run

use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig};

fn main() -> Result<(), nxuskit::NxuskitError> {
    // Create a provider — reads OPENAI_API_KEY from environment
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        ..Default::default()
    })?;

    // Build a chat request using the builder pattern
    let request = ChatRequest::new("gpt-4o-mini")
        .with_message(Message::user("What is the capital of France? Reply in one sentence."))
        .with_max_tokens(100);

    // Send the request
    let response = provider.chat(request)?;

    println!("Response: {}", response.content);

    println!(
        "Tokens: {} prompt + {} completion",
        response.usage.estimated.prompt_tokens,
        response.usage.estimated.completion_tokens,
    );

    Ok(())
}
