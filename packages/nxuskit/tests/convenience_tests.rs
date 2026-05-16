//! Tests for convenience methods (T032).
//!
//! Tests require the SDK runtime and are `#[ignore]`.

#[test]
#[ignore = "requires libnxuskit runtime"]
fn completion_returns_string() {
    let provider = nxuskit::builders::LoopbackProvider::builder()
        .build()
        .expect("loopback should build");
    let result = provider.completion("Say hello");
    assert!(result.is_ok(), "completion failed: {:?}", result.err());
    let text = result.unwrap();
    assert!(
        !text.is_empty(),
        "completion should return non-empty string"
    );
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn completion_stream_returns_receiver() {
    let provider = nxuskit::builders::LoopbackProvider::builder()
        .build()
        .expect("loopback should build");
    let result = provider.completion_stream("Say hello");
    assert!(
        result.is_ok(),
        "completion_stream failed: {:?}",
        result.err()
    );
}

#[test]
#[ignore = "requires libnxuskit runtime"]
fn completion_async_returns_string() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let provider = nxuskit::builders::LoopbackProvider::builder()
            .build()
            .expect("loopback should build");
        let result = provider.completion_async("Say hello").await;
        assert!(
            result.is_ok(),
            "completion_async failed: {:?}",
            result.err()
        );
        let text = result.unwrap();
        assert!(
            !text.is_empty(),
            "completion_async should return non-empty string"
        );
    });
}
