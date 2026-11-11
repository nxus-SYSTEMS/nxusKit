//! JSONL event sink used by the v1.1 contract-native runner.
//!
//! Internal to `nxuskit-engine`; the public CLI surfaces these events
//! over `stdout` when `pipeline run --format jsonl` is requested. The
//! event vocabulary is documented in the public JSONL event contract.

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;

/// One JSONL event line. Serializes as a single-line JSON object.
#[derive(Debug, Clone, Serialize)]
pub struct JsonlEvent {
    pub event: String,
    #[serde(flatten)]
    pub fields: serde_json::Value,
}

impl JsonlEvent {
    pub fn new(event: impl Into<String>, fields: serde_json::Value) -> Self {
        Self {
            event: event.into(),
            fields,
        }
    }

    /// Render as a single-line JSON string (no trailing newline).
    pub fn render(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Trait the runner uses to publish events. The CLI provides an
/// implementation that writes to stdout; tests provide a vec-backed
/// sink that captures events for assertion.
pub trait JsonlEventSink: Send + Sync {
    fn emit(&self, event: JsonlEvent);
}

/// No-op event sink. Useful as a default when JSONL streaming is off.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullEventSink;

impl JsonlEventSink for NullEventSink {
    fn emit(&self, _event: JsonlEvent) {}
}

/// Test event sink that captures emitted events into a `Vec`.
#[derive(Debug, Default, Clone)]
pub struct VecEventSink {
    inner: Arc<Mutex<Vec<JsonlEvent>>>,
}

impl VecEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<JsonlEvent> {
        self.inner.lock().clone()
    }
}

impl JsonlEventSink for VecEventSink {
    fn emit(&self, event: JsonlEvent) {
        self.inner.lock().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonl_event_renders_as_single_line_json() {
        let evt = JsonlEvent::new("stage_start", json!({"stage_id": "summarize"}));
        let line = evt.render();
        assert!(line.contains("\"event\":\"stage_start\""));
        assert!(line.contains("\"stage_id\":\"summarize\""));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn vec_event_sink_captures_emitted_events_in_order() {
        let sink = VecEventSink::new();
        sink.emit(JsonlEvent::new("pipeline_start", json!({})));
        sink.emit(JsonlEvent::new("pipeline_complete", json!({})));
        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "pipeline_start");
        assert_eq!(events[1].event, "pipeline_complete");
    }

    #[test]
    fn null_event_sink_discards_emitted_events() {
        let sink = NullEventSink;
        sink.emit(JsonlEvent::new("anything", json!({})));
        // No assertion needed; just that no panic.
    }
}
